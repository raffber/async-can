mod api;
mod sys;
use std::thread;
use crate::{Result, Error};
use crate::{Message, Timestamp};
use api::Handle;
use api::PCan;

const IOPORT: u32 = 0x02A0;
const INTERRUPT: u16 = 11;

struct RxThread {
    handle: Handle,
}

struct TxThread {

}

#[derive(Clone)]
pub struct PCanDevice {

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
            let num: u16 = usb_num.parse().map_err(|_| Error::InvalidInterfaceAddress)?;
            if num == 0 || num > 16 {
                return Err(Error::InvalidInterfaceAddress);
            } 
            num + 0x50
        } else { 
            return Err(Error::InvalidInterfaceAddress);
        };
        let baud = get_baud(bitrate)?;
        if let Err(err) = PCan::initalize(handle, baud as u16, sys::PCAN_TYPE_ISA as u8, IOPORT, INTERRUPT) {
            return Err(Error::PCanInitFailed(err.code, err.description())); 
        }
        Ok(Self {})
    }

    pub async fn send(&self, msg: Message) -> Result<()> {
        todo!()
    }

    pub async fn recv(&self) -> Result<Message> {
        todo!()
    }

    pub async fn recv_with_timestamp(&self) -> Result<(Message, Timestamp)> {
        todo!()
    }
}

