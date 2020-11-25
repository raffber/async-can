#![allow(dead_code)]

#[macro_use]
#[cfg(target_os = "windows")]
extern crate dlopen_derive;
#[macro_use]
#[cfg(target_os = "windows")]
extern crate lazy_static;

use serde::{Deserialize, Serialize};

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

#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
mod pcan;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
mod socketcan;
