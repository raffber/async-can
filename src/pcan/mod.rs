mod api;
mod sys;
use crate::{Error, Result};
use crate::{Message, Timestamp};
use api::Handle;
use api::PCan;
use std::thread;
use std::time::Duration;
use tokio::task;

const IOPORT: u32 = 0x02A0;
const INTERRUPT: u16 = 11;

#[derive(Clone)]
pub struct PCanDevice {
    handle: Handle,
}

fn get_baud(bitrate: u32) -> Result<u16> {
    let ret = match bitrate {
        5000 => sys::PCAN_BAUD_5K,
        10000 => sys::PCAN_BAUD_10K,
        20000 => sys::PCAN_BAUD_20K,
        33000 => sys::PCAN_BAUD_33K,
        47000 => sys::PCAN_BAUD_47K,
        50000 => sys::PCAN_BAUD_50K,
        83000 => sys::PCAN_BAUD_83K,
        95000 => sys::PCAN_BAUD_95K,
        100000 => sys::PCAN_BAUD_100K,
        125000 => sys::PCAN_BAUD_125K,
        250000 => sys::PCAN_BAUD_250K,
        500000 => sys::PCAN_BAUD_500K,
        800000 => sys::PCAN_BAUD_800K,
        1000000 => sys::PCAN_BAUD_1M,
        _ => return Err(Error::InvalidBitRate),
    };
    Ok(ret as u16)
}

impl PCanDevice {
    pub fn connect(ifname: &str, bitrate: u32) -> Result<Self> {
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
        let baud = get_baud(bitrate)?;
        if let Err(err) = PCan::initalize(
            handle,
            baud as u16,
            sys::PCAN_TYPE_ISA as u8,
            IOPORT,
            INTERRUPT,
        ) {
            return Err(Error::PCanInitFailed(err.code, err.description()));
        }
        // TODO: set PCAN_BUSOFF_AUTORESET PCAN_PARAMETER_ON
        Ok(Self { handle })
    }

    pub async fn send(&self, msg: Message) -> Result<()> {
        let handle = self.handle;
        // we unwrap because shouldn't panic
        task::spawn_blocking(move || {
            let msg = msg.into();
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

    pub async fn recv(&self) -> Result<Message> {
        self.recv_with_timestamp().await.map(|(msg, _)| msg)
    }

    pub async fn recv_with_timestamp(&self) -> Result<(Message, Timestamp)> {
        let handle = self.handle;
        let (msg, stamp) = task::spawn_blocking(move || {
            loop {
                let (err, data) = PCan::read(handle);
                let ret = if let Some((msg, stamp)) = data {
                    Ok((msg, stamp))
                } else if let Some(err) = err {
                    if err.other_error() != 0 {
                        Err(Error::PCanReadFailed(err.other_error(), err.description()))
                    } else if err.bus_error() != 0 {
                        Err(Error::BusError(api::parse_bus_error(err.bus_error())))
                    } else if err.rx_empty() {
                        // TODO: replace with event based rx
                        thread::sleep(Duration::from_millis(2));
                        continue;
                    } else {
                        Err(Error::PCanReadFailed(0, "Unknown error".to_string()))
                    }
                } else {
                    Err(Error::PCanReadFailed(0, "Unknown error".to_string()))
                };
                return ret;
            }
        })
        .await
        .unwrap()?;
        let msg: Message = msg.into();
        Ok((msg, stamp.into()))
    }
}
