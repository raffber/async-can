use std::ptr::{null, null_mut};

use super::api::PCan;
use crate::pcan::api::Handle;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Foundation::{CloseHandle, WAIT_FAILED};
use windows_sys::Win32::System::Threading::{CreateEventA, SetEvent, WaitForSingleObject};

pub(crate) struct Waiter {
    event_handle: HANDLE,
    cancel: Arc<AtomicBool>,
}

unsafe impl Send for Waiter {}

pub(crate) struct WaiterHandle {
    event_handle: HANDLE,
    cancel: Arc<AtomicBool>,
}
unsafe impl Send for WaiterHandle {}

impl Waiter {
    pub(crate) fn new(handle: Handle) -> crate::Result<(Self, WaiterHandle)> {
        let event_handle = unsafe {
            let handle = CreateEventA(null_mut(), 0, 0, null());
            if handle == 0 {
                panic!("CreateEventA failed. That should not happen...");
            }
            handle
        };
        PCan::register_event(handle, event_handle);
        log::debug!("Waiter Event registered");
        let cancel = Arc::new(AtomicBool::new(false));
        Ok((
            Waiter {
                event_handle,
                cancel: cancel.clone(),
            },
            WaiterHandle {
                event_handle,
                cancel,
            },
        ))
    }

    pub(crate) fn wait_for_event(&self) -> crate::Result<bool> {
        unsafe {
            let err = WaitForSingleObject(self.event_handle, 100);
            if err == WAIT_FAILED {
                panic!("Waiting for event has failed!");
            }
        }
        return Ok(self.cancel.load(Ordering::SeqCst));
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.event_handle);
        }
    }
}

impl WaiterHandle {
    pub(crate) fn close(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        unsafe {
            SetEvent(self.event_handle);
        }
    }
}
