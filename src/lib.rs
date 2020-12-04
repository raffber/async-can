#![allow(dead_code)]

#[macro_use]
#[cfg(target_os = "windows")]
extern crate dlopen_derive;
#[macro_use]
#[cfg(target_os = "windows")]
extern crate lazy_static;

use std::io;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg(target_os = "linux")]
pub use linux::{Sender, Receiver};
#[cfg(target_os = "windows")]
pub use windows::{Sender, Receiver};

pub const CAN_EXT_ID_MASK: u32 = 0x1FFFFFFF;
pub const CAN_STD_ID_MASK: u32 = 0x7FF;
pub const CAN_MAX_DLC: usize = 8;

#[derive(Serialize, Deserialize, Clone)]
pub struct DataFrame {
    id: u32,
    ext_id: bool,
    data: Vec<u8>,
}

impl DataFrame {
    pub fn id(&self) -> u32 { self.id }
    pub fn ext_id(&self) -> bool { self.ext_id }
    pub fn data(&self) -> &[u8] { &self.data }
    pub fn dlc(&self) -> u8 { self.data.len() as u8 }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RemoteFrame {
    id: u32,
    ext_id: bool,
    dlc: u8,
}

impl RemoteFrame {
    pub fn id(&self) -> u32 { self.id }
    pub fn ext_id(&self) -> bool { self.ext_id }
    pub fn dlc(&self) -> u8 { self.dlc }
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Message {
    Data(DataFrame),
    Remote(RemoteFrame),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Timestamp {
    pub micros: u64,
}

impl Message {
    pub fn new_data(id: u32, ext_id: bool, data: &[u8]) -> Option<Message> {
        if ext_id && id > CAN_EXT_ID_MASK {
            return None;
        } else if !ext_id && id > CAN_STD_ID_MASK {
            return None;
        }
        if data.len() > CAN_MAX_DLC {
            return None;
        }
        Some(Message::Data(DataFrame {
            id,
            ext_id,
            data: data.to_vec(),
        }))
    }

    pub fn new_remote(id: u32, ext_id: bool, dlc: u8) -> Option<Message> {
        if ext_id && id > CAN_EXT_ID_MASK {
            return None;
        } else if !ext_id && id > CAN_STD_ID_MASK {
            return None;
        }
        if dlc as usize > CAN_MAX_DLC {
            return None;
        }
        Some(Message::Remote(RemoteFrame {
            id,
            ext_id,
            dlc,
        }))
    }

    pub fn id(&self) -> u32 {
        match self {
            Message::Data(x) => x.id,
            Message::Remote(x) => x.id,
        }
    }

    pub fn ext_id(&self) -> bool {
        match self {
            Message::Data(x) => x.ext_id,
            Message::Remote(x) => x.ext_id,
        }
    }

    pub fn validate(&self) -> std::result::Result<(), CanFrameError> {
        if let Some(err) = CanFrameError::validate_id(self.id(), self.ext_id()) {
            return Err(err);
        }
        match self {
            Message::Data(frame) => {
                if frame.data.len() > 8 {
                    Err(CanFrameError::DataTooLong)
                } else {
                    Ok(())
                }
            }
            Message::Remote(frame) => {
                if frame.dlc > 8 {
                    Err(CanFrameError::DataTooLong)
                } else {
                    Ok(())
                }
            }
        }
    }
}

pub enum CanFrameError {
    IdTooLong,
    DataTooLong,
}

impl From<CanFrameError> for crate::Error {
    fn from(x: CanFrameError) -> Self {
        match x {
            CanFrameError::IdTooLong => Error::IdTooLong,
            CanFrameError::DataTooLong => Error::DataTooLong
        }
    }
}

impl CanFrameError {
    fn validate_id(id: u32, ext_id: bool) -> Option<CanFrameError> {
        if ext_id {
            if id > CAN_EXT_ID_MASK {
                return Some(CanFrameError::IdTooLong);
            }
        } else {
            if id > CAN_STD_ID_MASK {
                return Some(CanFrameError::IdTooLong);
            }
        }
        None
    }
}

#[derive(Error, Debug, Clone, Serialize, Deserialize)]
pub enum BusError {
    #[error("Bus-light warning")]
    LightWarning,
    #[error("Bus-heavy warning")]
    HeavyWarning,
    #[error("Bus in passive mode")]
    Passive,
    #[error("Bus is in bus-off mode")]
    Off,
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Io Error: {0}")]
    Io(io::Error),
    #[error("Invalid interface address")]
    InvalidInterfaceAddress,
    #[error("Invalid bitrate")]
    InvalidBitRate,
    #[error("PCAN Init Failed with code {0}: `{1}`")]
    PCanInitFailed(u32, String),
    #[error("Write failed with code {0}: `{1}`")]
    PCanWriteFailed(u32, String),
    #[error("Read failed with code {0}: `{1}`")]
    PCanReadFailed(u32, String),
    #[error("Bus error: {0}")]
    BusError(BusError),
    #[error("Transmit full")]
    TransmitQueueFull,
    #[error("Id is too long")]
    IdTooLong,
    #[error("Data is too long")]
    DataTooLong,
}

impl From<io::Error> for Error {
    fn from(x: io::Error) -> Self {
        Error::Io(x)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
mod pcan;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
mod socketcan;

