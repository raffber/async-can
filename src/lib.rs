#![allow(dead_code)]

#[macro_use]
#[cfg(target_os = "windows")]
extern crate dlopen_derive;
#[macro_use]
#[cfg(target_os = "windows")]
extern crate lazy_static;

use thiserror::Error;

use serde::{Deserialize, Serialize};
use std::io;


#[derive(Serialize, Deserialize, Clone)]
pub struct DataFrame {
    id: u32,
    ext_id: bool,
    data: Vec<u8>,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct RemoteFrame {
    id: u32,
    ext_id: bool,
    dlc: u8,
}


#[derive(Serialize, Deserialize, Clone)]
pub enum Message {
    Data(DataFrame),
    Remote(RemoteFrame),
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Timestamp {
    micros: u64
}

impl Message {
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
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("Io Error: {0}")]
    Io(io::Error),

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

#[cfg(target_os = "windows")]
pub use windows::Bus;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
mod socketcan;

#[cfg(target_os = "linux")]
pub use linux::Bus;

