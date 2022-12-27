//! This library provides a tokio based aynchronous IO stack for CAN communication.
//!
//! ## Listing to a CAN Bus
//!
//! ```no_run
//! # tokio_test::block_on(async {
//! use async_can::{pcan, socketcan};
//!
//! let mut receiver = pcan::Receiver::connect("usb1", 125000).unwrap();
//! // or: let receiver = socketcan::Receiver::connect("can0").unwrap();
//!
//! for _ in 0 .. 10 {
//!     let msg = receiver.recv().await;
//!     println!("Message Received: {:?}", msg);
//! }
//! # })
//! ```
//!
//! Note that all receivers implement the [`crate::Receiver`] trait.
//!
//!
//! ## Sending CAN Messages
//!
//! ```no_run
//! # tokio_test::block_on(async {
//! use async_can::{pcan, socketcan};
//! use async_can::Message;
//!
//! let mut sender = pcan::Sender::connect("usb1", 125000).unwrap();
//! // or: let sender = socketcan::Sender::connect("can0").unwrap();
//!
//! for k in 0 .. 10 {
//!     let msg = Message::new_data(/*id=*/ k | 0x100, /*ext_id=*/ true, /* data=*/ &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
//!     sender.send(msg).await.unwrap();
//! }
//! # })
//! ```
//!
//! Note that all senders implement the [`crate::Sender`] trait.
//!
//! ## USR CANNET devices
//!
//! USR CANET devices use a simple protocol on top of TCP to connect to a CAN bus.
//!
//! ```no_run
//! # tokio_test::block_on(async {
//! use async_can::usr_canet;
//!
//! let (sender, receiver) = usr_canet::connect("192.168.1.10:1").await.unwrap();
//! # });
//! ```
//!
//! ## Listing CAN devices
//!
//! ```no_run
//! # tokio_test::block_on(async {
//! use async_can::{pcan, socketcan};
//!
//! println!("{:?}", pcan::list_devices().await);
//! println!("{:?}", socketcan::list_devices().await);
//! # })
//! ```
//!
//! ## Serde Support
//!
//! ```toml
//! async-can = {version = "*", features = ["serde"]}
//! ```
//!
//! This allows serializing the [`Message`] and related type.
//!
//!
#![allow(dead_code)]

use async_trait::async_trait;
use std::io;
use std::result::Result as StdResult;
use thiserror::Error;

#[cfg(feature = "usr_canet")]
pub mod usr_canet;

pub mod loopback;

#[cfg(feature = "serde")]
use serde::{de::Error as SerdeDeError, Deserialize, Deserializer, Serialize};

/// Maximum value for CAN ID if extended 29-bit ID is selected
pub const CAN_EXT_ID_MASK: u32 = 0x1FFFFFFF;

/// Maximum value for CAN ID if standard 11-bit ID is selected
pub const CAN_STD_ID_MASK: u32 = 0x7FF;

/// Maximum data length or dlc in a CAN message
pub const CAN_MAX_DLC: usize = 8;

pub(crate) mod base {
    #[cfg(feature = "serde")]
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Eq, PartialEq)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub(crate) struct DataFrame {
        pub(crate) id: u32,
        pub(crate) ext_id: bool,
        pub(crate) data: Vec<u8>,
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub(crate) struct RemoteFrame {
        pub(crate) id: u32,
        pub(crate) ext_id: bool,
        pub(crate) dlc: u8,
    }
}

/// A CAN data frame, i.e. the RTR bit is set to 0
#[derive(Debug, Clone, Eq, PartialEq)]
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
    /// Create a new [`DataFrame`] and returns an error in case the ID is out of range or the data is too long.
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
    pub fn take_data(self) -> Vec<u8> {
        self.0.data
    }
}

/// A CAN remote frame, i.e. the RTR bit is set to 1. Also, this type of frame
///  does not have a data field.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct RemoteFrame(base::RemoteFrame);

impl RemoteFrame {
    /// Create a new [`RemoteFrame`] and returns an error in case the ID is out of range or the dlc is too long.
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

/// A timestamp which defines when the CAN message was received on the bus.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Timestamp {
    pub micros: u64,
}

/// A message on the CAN bus, either a [`DataFrame`] or a [`RemoteFrame`].
///
/// In the future this will also contain a CAN-FD frame type.
#[derive(Debug, Clone, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Message {
    Data(DataFrame),
    Remote(RemoteFrame),
}

impl Message {
    /// Create a new message containing a data frame. Returns an error in case the ID is out of range or the data is too long.
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

    /// Create a new message containing a remote frame. Returns an error in case the ID is out of range or the dlc is too long.
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

    pub fn dlc(&self) -> u8 {
        match self {
            Message::Data(x) => x.dlc(),
            Message::Remote(x) => x.0.dlc,
        }
    }
}

/// Encodes errors that may occur when attempting to create/validate CAN message fields.
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
        } else if id > CAN_STD_ID_MASK {
            return Err(CanFrameError::IdTooLong);
        }
        Ok(())
    }
}

/// This enum encodes errors/warning that may occur on the CAN bus
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

/// Error type encoding all possible errors that may occur in this crate
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

/// `#[async_trait]` that defines an interface to send CAN messages.
///
/// Useful for boxing up CAN Senders of different types
#[async_trait]
pub trait Sender {
    async fn send(&mut self, msg: Message) -> Result<()>;
}

/// `#[async_trait]` that defines an interface to receive CAN messages.
///
/// Useful for boxing up CAN Receivers of different types
#[async_trait]
pub trait Receiver {
    async fn recv(&mut self) -> Result<Message>;
}

#[cfg(feature = "pcan")]
pub mod pcan;

#[cfg(all(target_os = "linux", feature = "socket_can"))]
pub mod socketcan;

/// Captures CAN device information of devices connected to the host.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Deserialize))]
pub struct DeviceInfo {
    pub interface_name: String,
    pub is_ready: bool,
    pub index: u32,
}

#[cfg(test)]
mod test {
    use crate::CanFrameError;

    #[test]
    fn validate_id() {
        assert!(CanFrameError::validate_id(1 << 28, true).is_ok());
        assert!(matches!(
            CanFrameError::validate_id(1 << 29, true),
            Err(CanFrameError::IdTooLong)
        ));
        assert!(CanFrameError::validate_id(1 << 10, false).is_ok());
        assert!(matches!(
            CanFrameError::validate_id(1 << 11, false),
            Err(CanFrameError::IdTooLong)
        ));
    }
}
