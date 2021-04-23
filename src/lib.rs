#![allow(dead_code)]

#[macro_use]
extern crate dlopen_derive;
#[macro_use]
extern crate lazy_static;

use std::io;
use std::result::Result as StdResult;

use serde::de::Error as SerdeDeError;
use serde::{Deserialize, Deserializer, Serialize};
use thiserror::Error;

pub const CAN_EXT_ID_MASK: u32 = 0x1FFFFFFF;
pub const CAN_STD_ID_MASK: u32 = 0x7FF;
pub const CAN_MAX_DLC: usize = 8;

pub(crate) mod base {
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize, Serialize, Clone)]
    pub(crate) struct DataFrame {
        pub(crate) id: u32,
        pub(crate) ext_id: bool,
        pub(crate) data: Vec<u8>,
    }

    #[derive(Serialize, Deserialize, Clone)]
    pub(crate) struct RemoteFrame {
        pub(crate) id: u32,
        pub(crate) ext_id: bool,
        pub(crate) dlc: u8,
    }
}

#[derive(Serialize, Clone)]
pub struct DataFrame(base::DataFrame);

impl<'de> Deserialize<'de> for DataFrame {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        base::DataFrame::deserialize(deserializer).and_then(|x| {
            if CanFrameError::validate_id(x.id, x.ext_id).is_err() {
                return Err(D::Error::custom("CAN Id is too long"));
            }
            if x.data.len() > 8 {
                Err(D::Error::custom("Data field is too long"))
            } else {
                Ok(DataFrame(x))
            }
        })
    }
}

impl DataFrame {
    pub fn id(&self) -> u32 {
        self.0.id
    }
    pub fn ext_id(&self) -> bool {
        self.0.ext_id
    }
    pub fn data(&self) -> &[u8] {
        &self.0.data
    }
    pub fn dlc(&self) -> u8 {
        self.0.data.len() as u8
    }
}

#[derive(Serialize, Clone)]
pub struct RemoteFrame(base::RemoteFrame);

impl RemoteFrame {
    pub fn id(&self) -> u32 {
        self.0.id
    }
    pub fn ext_id(&self) -> bool {
        self.0.ext_id
    }
    pub fn dlc(&self) -> u8 {
        self.0.dlc
    }
}

impl<'de> Deserialize<'de> for RemoteFrame {
    fn deserialize<D>(deserializer: D) -> StdResult<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        base::RemoteFrame::deserialize(deserializer).and_then(|x| {
            if CanFrameError::validate_id(x.id, x.ext_id).is_err() {
                return Err(D::Error::custom("CAN Id is too long"));
            }
            if x.dlc > 8 {
                Err(D::Error::custom("DLC field is too long"))
            } else {
                Ok(RemoteFrame(x))
            }
        })
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Timestamp {
    pub micros: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Message {
    Data(DataFrame),
    Remote(RemoteFrame),
}

impl Message {
    pub fn new_data(id: u32, ext_id: bool, data: &[u8]) -> StdResult<Message, CanFrameError> {
        CanFrameError::validate_id(id, ext_id)?;
        if data.len() > CAN_MAX_DLC {
            return Err(CanFrameError::DataTooLong);
        }
        Ok(Message::Data(DataFrame(base::DataFrame {
            id,
            ext_id,
            data: data.to_vec(),
        })))
    }

    pub fn new_remote(id: u32, ext_id: bool, dlc: u8) -> StdResult<Message, CanFrameError> {
        CanFrameError::validate_id(id, ext_id)?;
        if dlc as usize > CAN_MAX_DLC {
            return Err(CanFrameError::DataTooLong);
        }
        Ok(Message::Remote(RemoteFrame(base::RemoteFrame {
            id,
            ext_id,
            dlc,
        })))
    }

    pub fn id(&self) -> u32 {
        match self {
            Message::Data(x) => x.0.id,
            Message::Remote(x) => x.0.id,
        }
    }

    pub fn ext_id(&self) -> bool {
        match self {
            Message::Data(x) => x.0.ext_id,
            Message::Remote(x) => x.0.ext_id,
        }
    }
}

#[derive(Debug)]
pub enum CanFrameError {
    IdTooLong,
    DataTooLong,
}

impl From<CanFrameError> for crate::Error {
    fn from(x: CanFrameError) -> Self {
        match x {
            CanFrameError::IdTooLong => Error::IdTooLong,
            CanFrameError::DataTooLong => Error::DataTooLong,
        }
    }
}

impl CanFrameError {
    fn validate_id(id: u32, ext_id: bool) -> StdResult<(), CanFrameError> {
        if ext_id {
            if id > CAN_EXT_ID_MASK {
                return Err(CanFrameError::IdTooLong);
            }
        } else {
            if id > CAN_STD_ID_MASK {
                return Err(CanFrameError::IdTooLong);
            }
        }
        Ok(())
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

pub mod pcan;

#[cfg(target_os = "linux")]
pub mod socketcan;
