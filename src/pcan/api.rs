#![allow(non_snake_case)]

use std::{
    ffi::CStr,
    ffi::c_void,
    io::Write,
    mem::{MaybeUninit, size_of},
};

#[cfg(target_os = "linux")]
use std::os::unix::prelude::RawFd;

use super::{DeviceInfo, sys};
use crate::{CanFrameError, Message};
use libloading::Library;
use std::os::raw::c_char;
use std::sync::LazyLock;
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

/// Generates the [`Api`] struct holding the loaded library and its function pointers.
///
/// Each function is also exposed as an `unsafe` method of the same name, so call sites read
/// like `pcan().api.CAN_Read(...)`.
macro_rules! pcan_api {
    ($($name:ident: fn($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)?;)*) => {
        struct Api {
            // Keeps the library loaded as long as the function pointers below are in use.
            _lib: Library,
            $($name: unsafe extern "C" fn($($ty),*) $(-> $ret)?,)*
        }

        impl Api {
            /// # Safety
            ///
            /// The library at `path` must export the declared symbols with matching signatures.
            unsafe fn load(path: &str) -> Result<Self, libloading::Error> {
                // SAFETY: see function-level safety comment.
                let lib = unsafe { Library::new(path)? };
                $(
                    // SAFETY: the declared signature matches the PCAN-Basic API.
                    let $name = unsafe {
                        *lib.get::<unsafe extern "C" fn($($ty),*) $(-> $ret)?>(stringify!($name))?
                    };
                )*
                Ok(Self { _lib: lib, $($name,)* })
            }

            $(
                unsafe fn $name(&self, $($arg: $ty),*) $(-> $ret)? {
                    // SAFETY: forwarded to the caller of this method.
                    unsafe { (self.$name)($($arg),*) }
                }
            )*
        }
    };
}

pcan_api! {
    CAN_Initialize: fn(channel: Handle, baud: Baudrate, hw_type: HwType, port: u32, interrupt: u16) -> Status;
    CAN_Uninitialize: fn(channel: Handle) -> Status;
    CAN_Reset: fn(channel: Handle) -> Status;
    CAN_GetStatus: fn(channel: Handle) -> Status;
    CAN_Read: fn(channel: Handle, msg: *mut PCanMessage, timestamp: *mut Timestamp) -> Status;
    CAN_Write: fn(channel: Handle, msg: *const PCanMessage) -> Status;
    CAN_GetErrorText: fn(error: Status, lang: u16, buf: *mut c_char) -> Status;
    CAN_SetValue: fn(channel: Handle, param: u8, buf: *const c_void, len: u32) -> Status;
    CAN_GetValue: fn(channel: Handle, param: u8, buf: *mut c_void, len: u32) -> Status;
}

fn path_str(path: &std::path::Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("Invalid PCAN library path: {}", path.display()))
}

/// 64-bit FNV-1a hash, used to name the extracted library after its contents.
#[cfg(target_os = "windows")]
fn fnv1a(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ byte as u64).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

static PCAN: LazyLock<Result<PCan, String>> = LazyLock::new(PCan::new);

/// Returns the loaded library.
///
/// Panics if loading failed. All public entry points call [`PCan::load()`] first, so this
/// cannot happen through the public API.
fn pcan() -> &'static PCan {
    PCAN.as_ref()
        .expect("PCAN library used without a successful PCan::load()")
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
                | sys::PCAN_ERROR_QXMTFULL
                | sys::PCAN_ERROR_OVERRUN
                | sys::PCAN_ERROR_QOVERRUN
                | sys::PCAN_ERROR_QRCVEMPTY)
    }

    pub fn rx_overflow(&self) -> bool {
        self.code & (sys::PCAN_ERROR_OVERRUN | sys::PCAN_ERROR_QOVERRUN) > 0
    }

    pub fn tx_overflow(&self) -> bool {
        self.code & (sys::PCAN_ERROR_XMTFULL | sys::PCAN_ERROR_QXMTFULL) > 0
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
    api: Api,
}

impl PCan {
    fn new() -> Result<Self, String> {
        let api = Self::load_bundled_library()?;
        Ok(PCan { api })
    }

    /// Extracts the bundled library to a temporary file, loads it and removes the file again.
    /// The loaded library stays mapped into memory.
    #[cfg(target_os = "linux")]
    fn load_bundled_library() -> Result<Api, String> {
        let mut file = NamedTempFile::new()
            .map_err(|err| format!("Cannot create temporary file for PCAN library: {err}"))?;
        file.write_all(PCAN_LIB)
            .map_err(|err| format!("Cannot extract PCAN library: {err}"))?;
        let api = unsafe { Api::load(path_str(file.path())?) }
            .map_err(|err| format!("Cannot load PCAN library: {err}"))?;
        // Dropping `file` deletes it.
        Ok(api)
    }

    /// Extracts the bundled library and loads it.
    ///
    /// A loaded DLL cannot be deleted on Windows, so it is extracted to a fixed path derived from
    /// its contents in the temp directory, and reused by later runs. This keeps at most one copy
    /// per library version.
    #[cfg(target_os = "windows")]
    fn load_bundled_library() -> Result<Api, String> {
        use std::{fs, path::PathBuf};

        let dir = std::env::temp_dir().join("async-can");
        let path: PathBuf = dir.join(format!("PCANBasic-{:016x}.dll", fnv1a(PCAN_LIB)));
        let is_extracted = || fs::read(&path).is_ok_and(|content| content == PCAN_LIB);
        if !is_extracted() {
            fs::create_dir_all(&dir)
                .map_err(|err| format!("Cannot create {}: {err}", dir.display()))?;
            // Write to a temporary file first and rename it, so that other processes never load a
            // partially written library.
            let mut file = NamedTempFile::new_in(&dir)
                .map_err(|err| format!("Cannot create temporary file for PCAN library: {err}"))?;
            file.write_all(PCAN_LIB)
                .map_err(|err| format!("Cannot extract PCAN library: {err}"))?;
            if let Err(err) = file.persist(&path) {
                // Another process may have extracted (and loaded) it concurrently.
                if !is_extracted() {
                    return Err(format!("Cannot extract PCAN library: {err}"));
                }
            }
        }
        unsafe { Api::load(path_str(&path)?) }
            .map_err(|err| format!("Cannot load PCAN library: {err}"))
    }

    /// Loads the PCAN library, if not done yet. Must be called before any other function.
    pub fn load() -> crate::Result<()> {
        match &*PCAN {
            Ok(_) => Ok(()),
            Err(err) => Err(crate::Error::PCanLibraryLoad(err.clone())),
        }
    }

    pub fn describe_status(status: u32) -> String {
        // The PCAN-Basic documentation requires a buffer of at least 256 bytes.
        let mut buf = [0_u8; 512];
        // Language 0 selects the system language, so the text may not be UTF-8.
        let result = unsafe {
            pcan()
                .api
                .CAN_GetErrorText(status, 0x00, buf.as_mut_ptr() as *mut c_char)
        };
        match CStr::from_bytes_until_nul(&buf) {
            Ok(text) if !text.is_empty() => text.to_string_lossy().into_owned(),
            _ => format!("Unknown PCAN status 0x{status:X} (error text unavailable: 0x{result:X})"),
        }
    }

    pub fn uninitialize(channel: Handle) -> Result<(), Error> {
        let status = unsafe { pcan().api.CAN_Uninitialize(channel) };
        Error::result(status)
    }

    pub fn initialize(
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
                pcan()
                    .api
                    .CAN_GetValue(channel, sys::PCAN_BUSSPEED_NOMINAL as u8, ptr, 4)
            }
        };
        // if status == sys::PCAN_ERROR_INITIALIZE all is good and we can just initialize the channel
        // if status == 0 means the channel is already initialized and we have to check bitrate
        if status == 0 {
            // implies channel initialized
            if current_speed != bitrate {
                let status = unsafe { pcan().api.CAN_Uninitialize(channel) };
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
            pcan()
                .api
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
            pcan().api.CAN_SetValue(
                channel,
                sys::PCAN_BUSOFF_AUTORESET as u8,
                &on as *const i32 as *const c_void,
                size_of::<i32>() as u32,
            )
        };
        Error::result(status)
    }

    #[cfg(target_os = "windows")]
    pub fn register_event(
        channel: Handle,
        event: windows_sys::Win32::Foundation::HANDLE,
    ) -> Result<(), Error> {
        let status = unsafe {
            // The driver expects a pointer to the event handle.
            let event_ptr =
                &event as *const windows_sys::Win32::Foundation::HANDLE as *const c_void;
            pcan().api.CAN_SetValue(
                channel,
                sys::PCAN_RECEIVE_EVENT as u8,
                event_ptr,
                size_of::<*const c_void>() as u32,
            )
        };
        Error::result(status)
    }

    /// Returns the event currently registered for `channel`, or null if there is none.
    #[cfg(target_os = "windows")]
    pub fn registered_event(
        channel: Handle,
    ) -> Result<windows_sys::Win32::Foundation::HANDLE, Error> {
        let mut event: windows_sys::Win32::Foundation::HANDLE = std::ptr::null_mut();
        let status = unsafe {
            pcan().api.CAN_GetValue(
                channel,
                sys::PCAN_RECEIVE_EVENT as u8,
                &mut event as *mut windows_sys::Win32::Foundation::HANDLE as *mut c_void,
                size_of::<windows_sys::Win32::Foundation::HANDLE>() as u32,
            )
        };
        Error::result(status)?;
        Ok(event)
    }

    pub fn reset(channel: Handle) -> Result<(), Error> {
        let status = unsafe { pcan().api.CAN_Reset(channel) };
        Error::result(status)
    }

    pub fn get_status(channel: Handle) -> Option<Error> {
        let status = unsafe { pcan().api.CAN_GetStatus(channel) };
        Error::new(status)
    }

    pub fn read(channel: Handle) -> (Option<Error>, Option<(PCanMessage, Timestamp)>) {
        // The driver only writes `msg` and `timestamp` if it read a message. Pre-set the message
        // type to "status", which is filtered out below, so nothing is forwarded otherwise.
        let mut msg = PCanMessage {
            id: 0,
            tp: sys::PCAN_MESSAGE_STATUS as u8,
            len: 0,
            data: [0; 8],
        };
        let mut timestamp = Timestamp {
            millis: 0,
            millis_overflow: 0,
            micros: 0,
        };
        let status = unsafe { pcan().api.CAN_Read(channel, &mut msg, &mut timestamp) };
        let err = Error::new(status);
        // Only forward standard, extended and RTR frames, no status or error frames.
        let frame_bits = (sys::PCAN_MESSAGE_EXTENDED | sys::PCAN_MESSAGE_RTR) as u8;
        if msg.tp & !frame_bits == 0 {
            (err, Some((msg, timestamp)))
        } else {
            (err, None)
        }
    }

    pub fn write(channel: Handle, msg: PCanMessage) -> Result<(), Error> {
        let status = unsafe { pcan().api.CAN_Write(channel, &msg as *const PCanMessage) };
        Error::result(status)
    }

    pub fn list_devices() -> Result<Vec<DeviceInfo>, Error> {
        let channel_info = MaybeUninit::<sys::TPCANChannelInformation>::uninit();
        let infos = unsafe {
            let mut channel_count = 0_u32;
            let status = pcan().api.CAN_GetValue(
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
            let status = pcan().api.CAN_GetValue(
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
            pcan().api.CAN_GetValue(
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
        // `millis` is a 32-bit counter that continues in `millis_overflow` when it wraps.
        let ms = ((val.millis_overflow as u64) << 32) | val.millis as u64;
        let micros = ms * 1000 + val.micros as u64;
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn describe_status() {
        if PCan::load().is_err() {
            // The bundled library is only available on x86-64.
            return;
        }
        assert!(!PCan::describe_status(sys::PCAN_ERROR_QRCVEMPTY).is_empty());
        // Unknown codes make CAN_GetErrorText fail.
        assert!(!PCan::describe_status(0xDEAD_BEEF).is_empty());
    }

    #[test]
    fn status_classification() {
        for code in [sys::PCAN_ERROR_XMTFULL, sys::PCAN_ERROR_QXMTFULL] {
            let err = Error::new(code).unwrap();
            assert_eq!(err.other_error(), 0, "{code:#x}");
            assert!(err.tx_overflow(), "{code:#x}");
        }
        for code in [sys::PCAN_ERROR_OVERRUN, sys::PCAN_ERROR_QOVERRUN] {
            let err = Error::new(code).unwrap();
            assert_eq!(err.other_error(), 0, "{code:#x}");
            assert!(err.rx_overflow(), "{code:#x}");
        }
        let err = Error::new(sys::PCAN_ERROR_QRCVEMPTY).unwrap();
        assert!(err.rx_empty() && err.other_error() == 0);
        let err = Error::new(sys::PCAN_ERROR_BUSOFF).unwrap();
        assert!(err.bus_error() != 0 && err.other_error() == 0);
        let err = Error::new(sys::PCAN_ERROR_ILLHW).unwrap();
        assert_ne!(err.other_error(), 0);
    }

    #[test]
    fn timestamp_conversion() {
        let ts = |millis, millis_overflow, micros| {
            crate::Timestamp::from(Timestamp {
                millis,
                millis_overflow,
                micros,
            })
            .micros
        };
        assert_eq!(ts(0, 0, 0), 0);
        assert_eq!(ts(1234, 0, 567), 1_234_567);
        // `millis` wraps after about 49.7 days and continues in `millis_overflow`.
        assert_eq!(ts(0, 1, 0), (1 << 32) * 1000);
        assert_eq!(ts(5, 2, 7), (2 << 32) * 1000 + 5_007);
    }
}
