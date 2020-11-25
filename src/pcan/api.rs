#![allow(non_snake_case)]

use std::{ffi::CStr, io::Write, mem::MaybeUninit};

use dlopen::wrapper::{Container, WrapperApi};
use tempfile::NamedTempFile;
use std::os::raw::c_char;
use lazy_static;
use crate::Message;
use super::sys;

const PCAN_LIB: &'static [u8] = include_bytes!("../../lib/PCANBasic.dll");

pub type Handle = u16;
pub type Status = u32;
pub type Parameter = u8;
pub type Device = u8;
pub type MessageType = u8;
pub type HwType = u8;
pub type Mode = u8;
pub type Baudrate = u16;

#[repr(C)]
pub struct PCanMessage {
    pub id: u32,
    pub tp: u8,
    pub len: u8, 
    pub data: [u8; 8],
}

#[repr(C)]
pub struct Timestamp {
    pub millis: u32,
    pub millis_overflow: u16,
    pub micros: u16,
}

#[derive(Clone, WrapperApi)]
struct Api {
    CAN_Initialize: unsafe extern "C" fn(channel: Handle, baud: Baudrate, hw_type: HwType, port: u32, interrupt: u16) -> Status,
    CAN_Uninitialize: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_Reset: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_GetStatus: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_Read: unsafe extern "C" fn(channel: Handle, msg: *mut PCanMessage, timestamp: *mut Timestamp) -> Status,
    CAN_Write: unsafe extern "C" fn(channel: Handle, msg: *const PCanMessage) -> Status,
    CAN_GetErrorText: unsafe extern "C" fn(error: Status, lang: u16, buf: *const c_char)
}

lazy_static! {
    static ref PCAN: PCan = PCan::new();
}


pub struct Error {
    pub code: u32,
}

impl Error {
    pub fn new(status: u32) -> Error {
        if status == 0 {
            panic!("Not an error");
        }
        Error {
            code: status,
        }
    }

    pub fn description(&self) -> String {
        PCan::describe_status(self.code)
    }

    pub fn result(status: u32) -> Result<()> {
        if status == 0 {
            Ok(())
        } else {
            Err(Error::new(status))
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub struct PCan {
    api: Container<Api>,
}


impl PCan {
    pub fn new() -> Self {
        let mut tmpfile = NamedTempFile::new().unwrap();
        tmpfile.write_all(PCAN_LIB).unwrap();
        let (_, path) = tmpfile.keep().unwrap();
        let name = path.to_str().unwrap();
        let api: Container<Api> = unsafe { Container::load(name) }.expect("Could not load PCan: Is the driver installed?");
        PCan {
            api,
        }
    } 

    pub fn describe_status(status: u32) -> String {
        unsafe {
            let mut data: [c_char; 512] = MaybeUninit::uninit().assume_init();
            PCAN.api.CAN_GetErrorText(status, 0x00, data.as_mut_ptr());
            let ret = CStr::from_ptr(data.as_ptr());
            ret.to_str().unwrap().to_string()
        }
    }

    pub fn initalize(channel: Handle, baud: Baudrate, hw_type: HwType, port: u32, interrupt: u16) -> Result<()> {
        let status = unsafe {
            PCAN.api.CAN_Initialize(channel, baud, hw_type, port, interrupt) 
        }; 
        Error::result(status)
    }

    pub fn uninitialize(channel: Handle) -> Result<()> {
        let status = unsafe {
            PCAN.api.CAN_Uninitialize(channel) 
        }; 
        Error::result(status) 
    }

    pub fn reset(channel: Handle) -> Result<()> {
        let status = unsafe {
            PCAN.api.CAN_Reset(channel) 
        }; 
        Error::result(status) 
    }

    pub fn get_status(channel: Handle) -> Option<Error> {
        let status = unsafe {
            PCAN.api.CAN_GetStatus(channel) 
        }; 
        if status == 0 {
            None
        } else {
            Some(Error::new(status))
        }
    }

    pub fn read(channel: Handle) -> Result<(PCanMessage, Timestamp)> {
        unsafe {
            let mut msg = MaybeUninit::<PCanMessage>::uninit();
            let mut timestamp = MaybeUninit::<Timestamp>::uninit();
            let status = PCAN.api.CAN_Read(channel, msg.as_mut_ptr(), timestamp.as_mut_ptr());
            if status == 0 {
                Ok((msg.assume_init(), timestamp.assume_init()))
            } else {
                Err(Error::new(status))
            }
        }
    }

    pub fn write(channel: Handle, msg: PCanMessage) -> Result<()> {
        let status = unsafe {
            PCAN.api.CAN_Write(channel, &msg as *const PCanMessage) 
        };
        Error::result(status)
    }
}

impl From<Message> for PCanMessage {
    fn from(msg: Message) -> Self {
        match msg {
            Message::Data(frame) => {
                let data = [0_u8, 8];
                data.copy_from_slice(&frame.data);
                let tp = 0_u8;
                let tp = if frame.ext_id {
                    sys::PCAN_MESSAGE_EXTENDED
                } else {
                    sys::PCAN_MESSAGE_STANDARD 
                }; 
                PCanMessage {
                    id: frame.id,
                    tp: tp as u8,
                    len: frame.data.len() as u8,
                    data, 
                }
            },
            Message::Remote(frame) => {
                let mut tp = if frame.ext_id {
                    sys::PCAN_MESSAGE_EXTENDED
                } else {
                    sys::PCAN_MESSAGE_STANDARD 
                }; 
                tp |= sys::PCAN_MESSAGE_RTR; 
                PCanMessage {
                    id: frame.id,
                    tp: tp as u8,
                    len: frame.dlc,
                    data: [0_u8, 8], 
                }
            } 
        }
    }
}