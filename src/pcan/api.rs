#![allow(non_snake_case)]

use std::{
    ffi::c_void,
    ffi::CStr,
    io::Write,
    mem::{size_of, MaybeUninit},
};

#[cfg(target_os = "linux")]
use std::os::unix::prelude::RawFd;

use super::{sys, DeviceInfo};
use crate::{CanFrameError, Message};
use dlopen::wrapper::{Container, WrapperApi};
use dlopen_derive::WrapperApi;
use lazy_static::lazy_static;
use std::os::raw::c_char;
use tempfile::NamedTempFile;

#[cfg(target_os = "windows")]
const PCAN_LIB: &[u8] = include_bytes!("../../lib/PCANBasic.dll");

#[cfg(target_os = "linux")]
const PCAN_LIB: &[u8] = include_bytes!("../../lib/libpcanbasic.so");

pub type Handle = u16;
pub type Status = u32;
pub type Parameter = u8;
pub type Device = u8;
pub type MessageType = u8;
pub type HwType = u8;
pub type Mode = u8;
pub type Baudrate = u16;

pub fn get_baud(bitrate: u32) -> crate::Result<u16> {
    let ret = match bitrate {
        5000 => sys::PCAN_BAUD_5K,
        10000 => sys::PCAN_BAUD_10K,
        20000 => sys::PCAN_BAUD_20K,
        33000 => sys::PCAN_BAUD_33K,
        47000 => sys::PCAN_BAUD_47K,
        50000 => sys::PCAN_BAUD_50K,
        83000 => sys::PCAN_BAUD_83K,
        95000 => sys::PCAN_BAUD_95K,
        100000 => sys::PCAN_BAUD_100K,
        125000 => sys::PCAN_BAUD_125K,
        250000 => sys::PCAN_BAUD_250K,
        500000 => sys::PCAN_BAUD_500K,
        800000 => sys::PCAN_BAUD_800K,
        1000000 => sys::PCAN_BAUD_1M,
        _ => return Err(crate::Error::InvalidBitRate),
    };
    Ok(ret as u16)
}

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
                data[0..frame.data().len()].copy_from_slice(frame.data());
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
    CAN_GetValue:
        unsafe extern "C" fn(channel: Handle, param: u8, buf: *mut c_void, len: u32) -> Status,
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
        let mut data: MaybeUninit<[c_char; 512]> = MaybeUninit::uninit();
        unsafe {
            PCAN.api
                .CAN_GetErrorText(status, 0x00, data.as_mut_ptr() as *mut c_char);
            let ret = CStr::from_ptr(data.as_ptr() as *const c_char);
            ret.to_str().unwrap().to_string()
        }
    }

    pub fn uninitialize(channel: Handle) -> Result<(), Error> {
        let status = unsafe { PCAN.api.CAN_Uninitialize(channel) };
        Error::result(status)
    }

    pub fn initalize(
        channel: Handle,
        bitrate: u32,
        hw_type: HwType,
        port: u32,
        interrupt: u16,
    ) -> Result<(), Error> {
        // already checked in caller
        let baud = get_baud(bitrate).unwrap();

        let mut current_speed: u32 = 0;
        let status = {
            let ptr = &mut current_speed as *mut u32 as *mut c_void;
            unsafe {
                PCAN.api
                    .CAN_GetValue(channel, sys::PCAN_BUSSPEED_NOMINAL as u8, ptr, 4)
            }
        };
        // if status == sys::PCAN_ERROR_INITIALIZE all is good and we can just initialize the channel
        // if status == 0 means the channel is already initialized and we have to check bitrate
        if status == 0 {
            // implies channel initialized
            if current_speed != bitrate {
                let status = unsafe { PCAN.api.CAN_Uninitialize(channel) };
                Error::result(status)?;
            }
        }

        if status != 0 {
            log::debug!(
                "Getting current bus speed failed: {}",
                Self::describe_status(status)
            );
        }
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

    #[cfg(target_os = "windows")]
    pub fn register_event(channel: Handle, event: isize) {
        unsafe {
            let event_int = event as usize;
            let event_ptr = &event_int as *const usize as *const c_void;
            let status = PCAN.api.CAN_SetValue(
                channel,
                sys::PCAN_RECEIVE_EVENT as u8,
                event_ptr,
                size_of::<*const c_void>() as u32,
            );
            if Error::result(status).is_err() {
                panic!("Cannot register event in driver: {}", status)
            }
        }
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

    pub fn list_devices() -> Result<Vec<DeviceInfo>, Error> {
        let channel_info = MaybeUninit::<sys::TPCANChannelInformation>::uninit();
        let infos = unsafe {
            let mut channel_count = 0_u32;
            let status = PCAN.api.CAN_GetValue(
                sys::PCAN_NONEBUS as u16,
                sys::PCAN_ATTACHED_CHANNELS_COUNT as u8,
                &mut channel_count as *mut u32 as *mut c_void,
                4,
            );
            Error::result(status)?;

            let mut infos = vec![channel_info; channel_count as usize];
            let ptr = infos.as_mut_ptr() as *mut c_void;
            let len = (channel_count as usize * std::mem::size_of::<sys::TPCANChannelInformation>())
                as u32;
            let status = PCAN.api.CAN_GetValue(
                sys::PCAN_NONEBUS as u16,
                sys::PCAN_ATTACHED_CHANNELS as u8,
                ptr,
                len,
            );
            Error::result(status)?;
            infos
        };
        Ok(infos
            .iter()
            .map(|x| unsafe {
                DeviceInfo {
                    handle: x.assume_init().channel_handle,
                }
            })
            .collect())
    }

    #[cfg(target_os = "linux")]
    pub fn get_fd(handle: Handle) -> Result<RawFd, Error> {
        use std::os::raw::c_int;

        let mut fd: c_int = 0;
        let status = unsafe {
            PCAN.api.CAN_GetValue(
                handle,
                sys::PCAN_RECEIVE_EVENT as u8,
                &mut fd as *mut c_int as *mut c_void,
                std::mem::size_of::<c_int>() as u32,
            )
        };
        Error::result(status)?;

        Ok(fd)
    }
}

impl From<Timestamp> for crate::Timestamp {
    fn from(val: Timestamp) -> Self {
        let us = val.micros as u64;
        let ms = val.millis as u64;
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
