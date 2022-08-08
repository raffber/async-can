mod api;
mod sys;
use crate::{Error, Result};
use crate::{Message, Timestamp};
use api::PCan;
use api::{Handle, PCanMessage};
use std::thread;
use tokio::sync::mpsc;
use tokio::task::{self, spawn_blocking};

use self::api::get_baud;

const IOPORT: u32 = 0x02A0;
const INTERRUPT: u16 = 11;

pub struct Receiver {
    handle: Handle,
    rx: mpsc::UnboundedReceiver<Result<(Message, Timestamp)>>,
}

#[cfg(target_os = "linux")]
mod waiter {
    use crate::pcan::api::Handle;
    use std::thread;
    use std::time::Duration;

    pub(crate) struct Waiter;

    impl Waiter {
        pub(crate) fn new(_handle: Handle) -> Self {
            Self
        }

        pub(crate) fn wait_for_event(&self) {
            thread::sleep(Duration::from_millis(2))
        }
    }
}

#[cfg(target_os = "windows")]
mod waiter {
    use std::ptr::{null, null_mut};

    use super::api::PCan;
    use crate::pcan::api::Handle;
    use winapi::um::{handleapi::CloseHandle, winnt::HANDLE};

    pub(crate) struct Waiter {
        event_handle: HANDLE,
    }

    impl Waiter {
        pub(crate) fn new(handle: Handle) -> Self {
            let event_handle = unsafe {
                let handle = winapi::um::synchapi::CreateEventA(null_mut(), 0, 0, null());
                if handle.is_null() {
                    panic!("CreateEventA failed. That should not happen...");
                }
                handle
            };
            PCan::register_event(handle, event_handle);
            log::debug!("Waiter Event registered");
            Waiter { event_handle }
        }

        pub(crate) fn wait_for_event(&self) {
            unsafe {
                let err = winapi::um::synchapi::WaitForSingleObject(self.event_handle, 100);
                if err == winapi::um::winbase::WAIT_FAILED {
                    panic!("Waiting for event has failed!");
                }
            }
        }
    }

    impl Drop for Waiter {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.event_handle);
            }
        }
    }
}

type Waiter = waiter::Waiter;

pub struct Sender {
    handle: Handle,
}

fn connect_handle(ifname: &str, bitrate: u32) -> Result<Handle> {
    let _ = get_baud(bitrate)?;
    let ifname = ifname.to_lowercase();
    let handle = if let Some(usb_num) = ifname.strip_prefix("usb") {
        let num: u16 = usb_num
            .parse()
            .map_err(|_| Error::InvalidInterfaceAddress)?;
        if num == 0 || num > 8 {
            return Err(Error::InvalidInterfaceAddress);
        }
        num + 0x50
    } else if let Some(pci_num) = ifname.strip_prefix("pci") {
        let num: u16 = pci_num
            .parse()
            .map_err(|_| Error::InvalidInterfaceAddress)?;
        if num == 0 || num > 8 {
            return Err(Error::InvalidInterfaceAddress);
        }
        num + 64
    } else {
        return Err(Error::InvalidInterfaceAddress);
    };
    if let Err(err) = PCan::initalize(handle, bitrate, sys::PCAN_TYPE_ISA as u8, IOPORT, INTERRUPT)
    {
        return Err(Error::PCanInitFailed(err.code, err.description()));
    }
    Ok(handle)
}

impl Sender {
    pub fn connect(ifname: &str, bitrate: u32) -> Result<Self> {
        let handle = connect_handle(ifname, bitrate)?;
        Ok(Self { handle })
    }

    pub async fn send(&self, msg: Message) -> Result<()> {
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

    pub async fn close(self) -> Result<()> {
        let handle = self.handle;
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
}

impl Receiver {
    pub fn connect(ifname: &str, bitrate: u32) -> Result<Self> {
        let handle = connect_handle(ifname, bitrate)?;
        let rx = Self::start_receive(handle);
        Ok(Self { handle, rx })
    }

    fn receive_iteration(handle: Handle, waiter: &Waiter) -> Option<Result<(Message, Timestamp)>> {
        let (err, data) = PCan::read(handle);
        if let Some(err) = err {
            if err.other_error() != 0 {
                Some(Err(Error::PCanReadFailed(
                    err.other_error(),
                    err.description(),
                )))
            } else if err.bus_error() != 0 {
                Some(Err(Error::BusError(api::parse_bus_error(err.bus_error()))))
            } else if err.rx_empty() || err.rx_overflow() {
                waiter.wait_for_event();
                None
            } else {
                Some(Err(Error::PCanReadFailed(err.code, err.description())))
            }
        } else if let Some((msg, timestamp)) = data {
            if let Ok(msg) = msg.into_message() {
                Some(Ok((msg, timestamp.into())))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn start_receive(handle: Handle) -> mpsc::UnboundedReceiver<Result<(Message, Timestamp)>> {
        let (tx, rx) = mpsc::unbounded_channel();
        thread::spawn(move || {
            let waiter = Waiter::new(handle);
            loop {
                if tx.is_closed() {
                    break;
                }
                if let Some(ret) = Self::receive_iteration(handle, &waiter) {
                    if tx.send(ret).is_err() {
                        break;
                    }
                }
            }
        });
        rx
    }

    pub async fn recv(&mut self) -> Result<Message> {
        self.recv_with_timestamp().await.map(|(msg, _)| msg)
    }

    pub async fn recv_with_timestamp(&mut self) -> Result<(Message, Timestamp)> {
        match self.rx.recv().await {
            Some(msg) => msg,
            None => Err(crate::Error::InvalidBitRate),
        }
    }

    pub async fn close(mut self) -> Result<()> {
        self.rx.close();
        Ok(())
    }
}

pub struct DeviceInfo {
    handle: Handle,
}

impl DeviceInfo {
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
        return Err(crate::Error::PCanUnknownInterfaceType(self.handle));
    }
}

pub async fn list_devices() -> crate::Result<Vec<DeviceInfo>> {
    spawn_blocking(move || PCan::list_devices())
        .await
        .unwrap()
        .map_err(|x| crate::Error::PCanOtherError(x.code, x.description()))
}
