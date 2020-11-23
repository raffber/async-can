use serde::{Serialize, Deserialize};

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


#[cfg(target_os="windows")]
mod windows;

#[cfg(target_os="linux")]
mod linux;

#[cfg(target_os="linux")]
mod socketcan;
