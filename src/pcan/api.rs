#![allow(non_snake_case)]

use std::{
    ffi::c_void,
    ffi::CStr,
    io::Write,
    mem::{size_of, MaybeUninit},
};

use super::sys;
use crate::{CanFrameError, Message};
use dlopen::wrapper::{Container, WrapperApi};
use lazy_static;
use std::os::raw::c_char;
use tempfile::NamedTempFile;

#[cfg(target_os = "windows")]
const PCAN_LIB: &'static [u8] = include_bytes!("../../lib/PCANBasic.dll");

#[cfg(target_os = "linux")]
const PCAN_LIB: &'static [u8] = include_bytes!("../../lib/libpcanbasic.so");

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

impl PCanMessage {
    pub fn from_message(msg: Message) -> Result<Self, CanFrameError> {
        CanFrameError::validate_id(msg.id(), msg.ext_id())?;
        match msg {
            Message::Data(frame) => {
                if frame.data().len() > 8 {
                    return Err(CanFrameError::DataTooLong);
                }

                let mut data = [0_u8; 8];
                data[0..frame.data().len()].copy_from_slice(&frame.data());
                let tp = if frame.ext_id() {
                    sys::PCAN_MESSAGE_EXTENDED
                } else {
                    sys::PCAN_MESSAGE_STANDARD
                };
                Ok(PCanMessage {
                    id: frame.id(),
                    tp: tp as u8,
                    len: frame.data().len() as u8,
                    data,
                })
            }
            Message::Remote(frame) => {
                if frame.dlc() > 8 {
                    return Err(CanFrameError::DataTooLong);
                }
                let mut tp = if frame.ext_id() {
                    sys::PCAN_MESSAGE_EXTENDED
                } else {
                    sys::PCAN_MESSAGE_STANDARD
                };
                tp |= sys::PCAN_MESSAGE_RTR;
                Ok(PCanMessage {
                    id: frame.id(),
                    tp: tp as u8,
                    len: frame.dlc(),
                    data: [0_u8; 8],
                })
            }
        }
    }

    pub fn into_message(self) -> crate::Result<Message> {
        let ext_id = (self.tp & sys::PCAN_MESSAGE_EXTENDED as u8) > 0;
        let rtr = self.tp & (sys::PCAN_MESSAGE_RTR as u8) > 0;
        if rtr {
            Ok(Message::new_remote(self.id, ext_id, self.len)?)
        } else {
            Ok(Message::new_data(
                self.id,
                ext_id,
                &self.data[0..self.len as usize],
            )?)
        }
    }
}

#[repr(C)]
pub struct Timestamp {
    pub millis: u32,
    pub millis_overflow: u16,
    pub micros: u16,
}

#[derive(Clone, WrapperApi)]
struct Api {
    CAN_Initialize: unsafe extern "C" fn(
        channel: Handle,
        baud: Baudrate,
        hw_type: HwType,
        port: u32,
        interrupt: u16,
    ) -> Status,
    CAN_Uninitialize: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_Reset: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_GetStatus: unsafe extern "C" fn(channel: Handle) -> Status,
    CAN_Read: unsafe extern "C" fn(
        channel: Handle,
        msg: *mut PCanMessage,
        timestamp: *mut Timestamp,
    ) -> Status,
    CAN_Write: unsafe extern "C" fn(channel: Handle, msg: *const PCanMessage) -> Status,
    CAN_GetErrorText: unsafe extern "C" fn(error: Status, lang: u16, buf: *const c_char),
    CAN_SetValue:
        unsafe extern "C" fn(channel: Handle, param: u8, buf: *const c_void, len: u32) -> Status,
}

lazy_static! {
    static ref PCAN: PCan = PCan::new();
}

pub struct Error {
    pub code: u32,
}

impl Error {
    pub fn new(status: u32) -> Option<Error> {
        if status == 0 {
            None
        } else {
            Some(Error { code: status })
        }
    }

    pub fn description(&self) -> String {
        PCan::describe_status(self.code)
    }

    pub fn bus_error(&self) -> u32 {
        self.code & sys::PCAN_ERROR_ANYBUSERR
    }

    pub fn other_error(&self) -> u32 {
        self.code
            & !(sys::PCAN_ERROR_ANYBUSERR
                | sys::PCAN_ERROR_XMTFULL
                | sys::PCAN_ERROR_XMTFULL
                | sys::PCAN_ERROR_OVERRUN
                | sys::PCAN_ERROR_QRCVEMPTY)
    }

    pub fn rx_overflow(&self) -> bool {
        self.code & sys::PCAN_ERROR_OVERRUN > 0
    }

    pub fn tx_overflow(&self) -> bool {
        self.code & sys::PCAN_ERROR_XMTFULL > 0
    }

    pub fn rx_empty(&self) -> bool {
        self.code & sys::PCAN_ERROR_QRCVEMPTY > 0
    }

    pub fn result(status: u32) -> Result<(), Error> {
        if status == 0 {
            Ok(())
        } else {
            Err(Error::new(status).unwrap())
        }
    }
}

pub struct PCan {
    api: Container<Api>,
}

impl PCan {
    pub fn new() -> Self {
        let mut tmpfile = NamedTempFile::new().unwrap();
        tmpfile.write_all(PCAN_LIB).unwrap();
        let (_, path) = tmpfile.keep().unwrap();
        let name = path.to_str().unwrap();
        let api: Container<Api> = unsafe { Container::load(name) }
            .expect("Could not load PCan: Is the driver installed?");
        PCan { api }
    }

    pub fn describe_status(status: u32) -> String {
        unsafe {
            let mut data: [c_char; 512] = MaybeUninit::uninit().assume_init();
            PCAN.api.CAN_GetErrorText(status, 0x00, data.as_mut_ptr());
            let ret = CStr::from_ptr(data.as_ptr());
            ret.to_str().unwrap().to_string()
        }
    }

    pub fn initalize(
        channel: Handle,
        baud: Baudrate,
        hw_type: HwType,
        port: u32,
        interrupt: u16,
    ) -> Result<(), Error> {
        let status = unsafe {
            PCAN.api
                .CAN_Initialize(channel, baud, hw_type, port, interrupt)
        };
        if status == sys::PCAN_ERROR_INITIALIZE {
            // already initialized, maybe...
            // TODO: how to tell exactly if this is an error?
            return Ok(());
        }
        Error::result(status)?;
        let status = unsafe {
            let on = sys::PCAN_PARAMETER_ON as i32;
            PCAN.api.CAN_SetValue(
                channel,
                sys::PCAN_BUSOFF_AUTORESET as u8,
                &on as *const i32 as *const c_void,
                size_of::<i32>() as u32,
            )
        };
        Error::result(status)
    }

    pub fn register_event(channel: Handle, event: *const c_void) {
        unsafe {
            let status = PCAN.api.CAN_SetValue(
                channel,
                sys::PCAN_RECEIVE_EVENT as u8,
                event,
                size_of::<*const c_void>() as u32,
            );
            if Error::result(status).is_err() {
                panic!("Cannot register event in driver.")
            }
        }
    }

    pub fn uninitialize(channel: Handle) -> Result<(), Error> {
        let status = unsafe { PCAN.api.CAN_Uninitialize(channel) };
        Error::result(status)
    }

    pub fn reset(channel: Handle) -> Result<(), Error> {
        let status = unsafe { PCAN.api.CAN_Reset(channel) };
        Error::result(status)
    }

    pub fn get_status(channel: Handle) -> Option<Error> {
        let status = unsafe { PCAN.api.CAN_GetStatus(channel) };
        Error::new(status)
    }

    pub fn read(channel: Handle) -> (Option<Error>, Option<(PCanMessage, Timestamp)>) {
        let (err, msg, timestamp) = unsafe {
            let mut msg = MaybeUninit::<PCanMessage>::uninit();
            let mut timestamp = MaybeUninit::<Timestamp>::uninit();
            let status = PCAN
                .api
                .CAN_Read(channel, msg.as_mut_ptr(), timestamp.as_mut_ptr());
            let msg = msg.assume_init();
            let timestamp = timestamp.assume_init();
            (Error::new(status), msg, timestamp)
        };
        if msg.tp & 0x03 > 0 || msg.tp == 0 {
            // rtr, std, ext
            (err, Some((msg, timestamp)))
        } else {
            (err, None)
        }
    }

    pub fn write(channel: Handle, msg: PCanMessage) -> Result<(), Error> {
        let status = unsafe { PCAN.api.CAN_Write(channel, &msg as *const PCanMessage) };
        Error::result(status)
    }
}

impl Into<crate::Timestamp> for Timestamp {
    fn into(self) -> crate::Timestamp {
        let us = self.micros as u64;
        let ms = self.millis as u64;
        let micros = ms * 1000 + us;
        crate::Timestamp { micros }
    }
}

pub fn parse_bus_error(err: u32) -> crate::BusError {
    if err & sys::PCAN_ERROR_BUSOFF > 0 {
        crate::BusError::Off
    } else if err & sys::PCAN_ERROR_BUSPASSIVE > 0 {
        crate::BusError::Passive
    } else if err & sys::PCAN_ERROR_BUSHEAVY > 0 {
        crate::BusError::HeavyWarning
    } else if err & sys::PCAN_ERROR_BUSLIGHT > 0 {
        crate::BusError::LightWarning
    } else {
        panic!("No bus-error flag: {:x}", err);
    }
}
