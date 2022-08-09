#![allow(dead_code)]

#[macro_use]
extern crate dlopen_derive;
#[macro_use]
extern crate lazy_static;

use async_trait::async_trait;
use std::io;
use std::result::Result as StdResult;
use thiserror::Error;

#[cfg(feature = "serde")]
use serde::{de::Error as SerdeDeError, Deserialize, Deserializer, Serialize};

pub const CAN_EXT_ID_MASK: u32 = 0x1FFFFFFF;
pub const CAN_STD_ID_MASK: u32 = 0x7FF;
pub const CAN_MAX_DLC: usize = 8;

pub(crate) mod base {
    #[cfg(feature = "serde")]
    use serde::{Deserialize, Serialize};

    #[derive(Clone)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub(crate) struct DataFrame {
        pub(crate) id: u32,
        pub(crate) ext_id: bool,
        pub(crate) data: Vec<u8>,
    }

    #[derive(Clone)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub(crate) struct RemoteFrame {
        pub(crate) id: u32,
        pub(crate) ext_id: bool,
        pub(crate) dlc: u8,
    }
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct DataFrame(base::DataFrame);

#[cfg(feature = "serde")]
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
    pub fn new(id: u32, ext_id: bool, data: Vec<u8>) -> StdResult<Self, CanFrameError> {
        CanFrameError::validate_id(id, ext_id)?;
        if data.len() > CAN_MAX_DLC {
            return Err(CanFrameError::DataTooLong);
        }
        Ok(Self(base::DataFrame { id, ext_id, data }))
    }

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

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct RemoteFrame(base::RemoteFrame);

impl RemoteFrame {
    pub fn new(id: u32, ext_id: bool, dlc: u8) -> StdResult<Self, CanFrameError> {
        CanFrameError::validate_id(id, ext_id)?;
        if dlc as usize > CAN_MAX_DLC {
            return Err(CanFrameError::DataTooLong);
        }
        Ok(Self(base::RemoteFrame { id, ext_id, dlc }))
    }

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

#[cfg(feature = "serde")]
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

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Timestamp {
    pub micros: u64,
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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

#[derive(Error, Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
    #[error("Interface type was not recognized: {0}")]
    PCanUnknownInterfaceType(u16),
    #[error("Other PCAN Error {0}: `{1}`")]
    PCanOtherError(u32, String),
    #[error("Other Error: {0}")]
    Other(String),
}

impl From<io::Error> for Error {
    fn from(x: io::Error) -> Self {
        Error::Io(x)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[async_trait]
pub trait Sender {
    async fn send(&mut self, msg: Message) -> Result<()>;
}

#[async_trait]
pub trait Receiver {
    async fn recv(&mut self) -> Result<Message>;
}

#[cfg(feature = "pcan")]
pub mod pcan;

#[cfg(all(target_os = "linux", feature = "socket_can"))]
pub mod socketcan;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct DeviceInfo {
    pub interface_name: String,
    pub is_ready: bool,
}

pub async fn list_devices() -> crate::Result<Vec<DeviceInfo>> {
    #[cfg(target_os = "windows")]
    {
        let interfaces = pcan::list_devices().await?;
        let mut ret = Vec::new();
        for device_info in interfaces {
            let device_info = DeviceInfo {
                interface_name: device_info.interface_name()?,
                is_ready: true,
            };
            ret.push(device_info);
        }
        return Ok(ret);
    }
    #[cfg(target_os = "linux")]
    {
        return socketcan::list_devices().await;
    }
}
