mod api;
mod sys;
use crate::{Error, Result};
use crate::{Message, Timestamp};
use api::PCan;
use api::{Handle, PCanMessage};
use std::sync::Mutex;
use std::{sync::Arc, thread};
use tokio::sync::mpsc;
use tokio::task;

use self::api::get_baud;

const IOPORT: u32 = 0x02A0;
const INTERRUPT: u16 = 11;

pub struct Receiver {
    handle: Handle,
    rx: mpsc::UnboundedReceiver<Result<(Message, Timestamp)>>,
    cancel: Arc<Mutex<bool>>,
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
        if num == 0 || num > 16 {
            return Err(Error::InvalidInterfaceAddress);
        }
        num + 0x50
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
        let (rx, cancel) = Self::start_receive(handle);
        Ok(Self { handle, rx, cancel })
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

    pub fn start_receive(
        handle: Handle,
    ) -> (
        mpsc::UnboundedReceiver<Result<(Message, Timestamp)>>,
        Arc<Mutex<bool>>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let cancel = Arc::new(Mutex::new(false));
        let cancel_ret = cancel.clone();
        thread::spawn(move || {
            let waiter = Waiter::new(handle);
            loop {
                {
                    let cancel = cancel.lock().unwrap();
                    if *cancel {
                        break;
                    }
                }
                if let Some(ret) = Self::receive_iteration(handle, &waiter) {
                    if tx.send(ret).is_err() {
                        break;
                    }
                }
            }
        });
        (rx, cancel_ret)
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
        if let Ok(mut cancel) = self.cancel.lock() {
            *cancel = true;
        }
        while let Some(_) = self.rx.recv().await {}
        Ok(())
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        if let Ok(mut cancel) = self.cancel.lock() {
            *cancel = true;
        }
    }
}
