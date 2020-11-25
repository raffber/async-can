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
use std::sync::Arc;

fn is_false(x: &bool) -> bool {
    !(*x)
}


#[derive(Serialize, Deserialize, Clone)]
pub struct DataFrame {
    id: u32,
    #[serde(skip_serializing_if = "is_false", default)]
    ext_id: bool,
    data: Vec<u8>,
}


#[derive(Serialize, Deserialize, Clone)]
pub struct RemoteFrame {
    id: u32,
    #[serde(skip_serializing_if = "is_false", default)]
    ext_id: bool,
    dlc: u8,
}


#[derive(Serialize, Deserialize, Clone)]
pub enum Message {
    Data(DataFrame),
    Remote(RemoteFrame),
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

#[derive(Error, Debug, Clone)]
pub enum Error {
    #[error("Io Error: {0}")]
    Io(Arc<io::Error>),

}

impl From<io::Error> for Error {
    fn from(x: io::Error) -> Self {
        Error::Io(Arc::new(x))
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
