#![allow(non_snake_case)]

use std::io::Write;

use dlopen::wrapper::{Container, WrapperApi};
use tempfile::NamedTempFile;
use std::os::raw::c_char;
use lazy_static;

const PCAN_LIB: &'static [u8] = include_bytes!("../../lib/PCANBasic.dll");

type Handle = u16;
type Status = u32;
type Parameter = u8;
type Device = u8;
type MessageType = u8;
type HwType = u8;
type Mode = u8;
type Baudrate = u16;

#[repr(C)]
struct Message {}

#[repr(C)]
struct Timestamp {}

#[derive(Clone, WrapperApi)]
struct Api {
    CAN_Initialize: unsafe extern "C" fn(channel: Handle, baud: Baudrate, hw_type: HwType, port: u32, interrupt: u16) -> Status,
    CAN_Uninitialize: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_Reset: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_GetStatus: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_Read: unsafe extern "C" fn(channel: Handle, msg: *mut Message, timestamp: *mut Timestamp) -> Status,
    CAN_Write: unsafe extern "C" fn(channel: Handle, msg: *const Message) -> Status,
    CAN_GetErrorText: unsafe extern "C" fn(error: Status, lang: u16, buf: *const c_char)
}

lazy_static! {
    static ref PCAN: PCan = PCan::new();
}

struct PCan {
    api: Container<Api>,
}

impl PCan {
    fn new() -> Self {
        let mut tmpfile = NamedTempFile::new().unwrap();
        tmpfile.write_all(PCAN_LIB).unwrap();
        let (_, path) = tmpfile.keep().unwrap();
        let name = path.to_str().unwrap();
        let api: Container<Api> = unsafe { Container::load(name) }.expect("Could not load PCan: Is the driver installed?");
        PCan {
            api
        }
    }
}
