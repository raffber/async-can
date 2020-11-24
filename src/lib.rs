#![allow(dead_code)]

#[macro_use]
#[cfg(target_os="windows")]
extern crate dlopen_derive;
#[macro_use]
#[cfg(target_os="windows")]
extern crate lazy_static;

use serde::{Deserialize, Serialize};

fn is_false(x: &bool) -> bool {
    !(*x)
}

#[derive(Serialize, Deserialize, Clone)]
pub struct CanMessage {
    id: u32,

    #[serde(skip_serializing_if = "is_false", default)]
    ext_id: bool,

    #[serde(skip_serializing_if = "is_false", default)]
    rtr: bool,

    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    data: Vec<u8>,
}


#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "windows")]
mod pcan;

#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
mod socketcan;
