//! Implements an async interface to the PCAN-API
//!
//! This modules ships directly with the necessary DLL embedded and
//! automatically extracts and loads it the first time you use it.
//! However, the [PCAN driver](https://www.peak-system.com/Drivers.523.0.html) still needs to be installed.
//!
//! Note that on linux it is generally recommended to use the SocketCAN interface. With most recent distributions
//! no installation is required.
//!
//!
//! ## Interface Names
//!
//! To reduce interface differences between the SocketCAN module and the PCAN module, we use string based
//! device names to identify the PCAN devices. Currently the following devices are supported:
//!
//!  * "usb1" to "usb8"
//!  * "pci0" to "pci8"
//!
//! If you know that only a single USB dongle will be connected to the host, it's safe to just hard-code the "usb1" string.
//!

mod api;
mod sys;
use crate::{Error, Result};
use crate::{Message, Timestamp};
use api::PCan;
use api::{Handle, PCanMessage};
use async_trait::async_trait;
use std::thread;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task::{self, spawn_blocking};

use self::api::get_baud;

const IOPORT: u32 = 0x02A0;
const INTERRUPT: u16 = 11;

#[cfg(target_os = "linux")]
mod waiter_linux;

#[cfg(target_os = "linux")]
use waiter_linux::{Waiter, WaiterHandle};

#[cfg(target_os = "windows")]
mod waiter_windows;

#[cfg(target_os = "windows")]
use waiter_windows::{Waiter, WaiterHandle};

fn parse_ifname(ifname: &str) -> Result<Handle> {
    let ifname = ifname.to_lowercase();
    if let Some(usb_num) = ifname.strip_prefix("usb") {
        let num: u16 = usb_num
            .parse()
            .map_err(|_| Error::InvalidInterfaceAddress)?;
        if num == 0 || num > 8 {
            return Err(Error::InvalidInterfaceAddress);
        }
        Ok(num + 0x50)
    } else if let Some(pci_num) = ifname.strip_prefix("pci") {
        let num: u16 = pci_num
            .parse()
            .map_err(|_| Error::InvalidInterfaceAddress)?;
        if num == 0 || num > 8 {
            return Err(Error::InvalidInterfaceAddress);
        }
        Ok(num + 64)
    } else {
        Err(Error::InvalidInterfaceAddress)
    }
}

fn connect_handle(ifname: &str, bitrate: u32) -> Result<Handle> {
    let _ = get_baud(bitrate)?;
    let handle = parse_ifname(ifname)?;
    if let Err(err) = PCan::initalize(handle, bitrate, sys::PCAN_TYPE_ISA as u8, IOPORT, INTERRUPT)
    {
        return Err(Error::PCanInitFailed(err.code, err.description()));
    }
    Ok(handle)
}

/// Attempt de-initialize an interface, thus disconnecting from the CAN bus
pub async fn deinitialize(ifname: &str) -> Result<()> {
    let handle = parse_ifname(ifname)?;
    task::spawn_blocking(move || match PCan::uninitialize(handle) {
        Ok(_) => Ok(()),
        Err(err) => {
            if err.code == sys::PCAN_ERROR_INITIALIZE {
                Ok(())
            } else {
                Err(crate::Error::PCanInitFailed(err.code, err.description()))
            }
        }
    })
    .await
    .unwrap()
}

/// Allows sending messages to the CAN bus.
pub struct Sender {
    handle: Handle,
}

impl Sender {
    /// Connect the given interface and initializes the adapter to the given bitrate (if required).
    /// For nameing interafaces, refer to the [module documentation](crate::pcan).
    pub fn connect(ifname: &str, bitrate: u32) -> Result<Self> {
        let handle = connect_handle(ifname, bitrate)?;
        Ok(Self { handle })
    }

    /// Send a message to the CAN bus
    pub async fn send(&mut self, msg: Message) -> Result<()> {
        let handle = self.handle;
        // we unwrap because shouldn't panic
        task::spawn_blocking(move || {
            let msg = PCanMessage::from_message(msg)?;
            match PCan::write(handle, msg) {
                Err(err) => {
                    if err.other_error() != 0 {
                        let err = api::Error::new(err.other_error()).unwrap();
                        Err(Error::PCanWriteFailed(err.code, err.description()))
                    } else if err.bus_error() != 0 {
                        Err(Error::BusError(api::parse_bus_error(err.bus_error())))
                    } else if err.tx_overflow() {
                        Err(Error::TransmitQueueFull)
                    } else {
                        Err(Error::PCanWriteFailed(0, "Unknown Error".to_string()))
                    }
                }
                Ok(x) => Ok(x),
            }
        })
        .await
        .unwrap()
    }
}

#[async_trait]
impl crate::Sender for Sender {
    async fn send(&mut self, msg: Message) -> Result<()> {
        self.send(msg).await
    }
}

/// Allows receiving message from the CAN bus.
pub struct Receiver {
    handle: Handle,
    rx: mpsc::UnboundedReceiver<Result<(Message, Timestamp)>>,
    waiter_handle: WaiterHandle,
}

impl Receiver {
    /// Connect the given interface and initializes the adapter to the given bitrate (if required).
    /// For nameing interafaces, refer to the [module documentation](crate::pcan).
    pub fn connect(ifname: &str, bitrate: u32) -> Result<Self> {
        let handle = connect_handle(ifname, bitrate)?;
        Self::start_receive(handle)
    }

    fn receive_loop(
        handle: Handle,
        waiter: Waiter,
        tx: UnboundedSender<crate::Result<(Message, Timestamp)>>,
    ) {
        loop {
            if tx.is_closed() {
                log::debug!("Channel closed, quitting.");
                break;
            }
            let (err, data) = PCan::read(handle);
            let to_send = match err {
                Some(err) if err.other_error() != 0 => Some(Err(Error::PCanReadFailed(
                    err.other_error(),
                    err.description(),
                ))),
                Some(err) if err.rx_empty() | err.rx_overflow() => match waiter.wait_for_event() {
                    Ok(false) => continue,
                    Ok(true) => {
                        log::debug!("Waker cancelled!");
                        break;
                    }
                    Err(x) => {
                        log::debug!("Error occurred, quitting receiver: {:?}", x);
                        let _ = tx.send(Err(x)).is_err();
                        break;
                    }
                },
                Some(err) => Some(Err(Error::PCanReadFailed(err.code, err.description()))),
                None => None,
            };
            if let Some(x) = to_send {
                if tx.send(x).is_err() {
                    log::debug!("Channel closed, quitting.");
                    break;
                }
            }
            if let Some((msg, timestamp)) = data {
                if let Ok(msg) = msg.into_message() {
                    if tx.send(Ok((msg, timestamp.into()))).is_err() {
                        log::debug!("Channel closed, quitting.");
                        break;
                    }
                }
            }
        }
        log::debug!("Leaving receiver.");
    }

    fn start_receive(handle: Handle) -> crate::Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let (waiter, waiter_handle) = Waiter::new(handle)?;
        thread::spawn(move || Self::receive_loop(handle, waiter, tx));
        Ok(Self {
            rx,
            handle,
            waiter_handle,
        })
    }

    /// Try to receive a message from the CAN bus
    pub async fn recv(&mut self) -> Result<Message> {
        self.recv_with_timestamp().await.map(|(msg, _)| msg)
    }

    /// Try to receive a message from the CAN bus, returning a message and an associated [crate::Timestamp] when the
    /// message was received.
    pub async fn recv_with_timestamp(&mut self) -> Result<(Message, Timestamp)> {
        match self.rx.recv().await {
            Some(msg) => msg,
            None => Err(crate::Error::Other("Receiver disconnected.".to_string())),
        }
    }

    /// Close the device and drop the underlying handle.
    pub fn close(mut self) -> Result<()> {
        self.rx.close();
        Ok(())
    }
}

#[async_trait]
impl crate::Receiver for Receiver {
    async fn recv(&mut self) -> Result<Message> {
        self.recv().await
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.waiter_handle.close();
    }
}

/// Collects information of devices connected to the host
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    handle: Handle,
}

impl DeviceInfo {
    /// Returns the interface name that can be passed to [`Receiver`] and [`Sender`] types.
    /// For more information on interface names, refer to the [module documentation](crate::pcan).
    pub fn interface_name(&self) -> crate::Result<String> {
        if self.handle >= sys::PCAN_USBBUS1 as Handle && self.handle <= sys::PCAN_USBBUS8 as Handle
        {
            let num = self.handle - sys::PCAN_USBBUS1 as Handle + 1;
            return Ok(format!("usb{}", num));
        } else if self.handle >= sys::PCAN_PCIBUS1 as Handle
            && self.handle <= sys::PCAN_PCIBUS8 as Handle
        {
            let num = self.handle - sys::PCAN_PCIBUS1 as Handle + 1;
            return Ok(format!("pci{}", num));
        }
        Err(crate::Error::PCanUnknownInterfaceType(self.handle))
    }
}

/// Retrieve all PCAN devices connected the host
pub async fn list_devices() -> crate::Result<Vec<DeviceInfo>> {
    spawn_blocking(PCan::list_devices)
        .await
        .unwrap()
        .map_err(|x| crate::Error::PCanOtherError(x.code, x.description()))
}
