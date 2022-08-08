use std::ptr::{null, null_mut};

use super::api::PCan;
use crate::pcan::api::Handle;
use winapi::um::{handleapi::CloseHandle, winnt::HANDLE};

pub(crate) struct Waiter {
    event_handle: HANDLE,
}

impl Waiter {
    pub(crate) fn new(handle: Handle) -> Self {
        let event_handle = unsafe {
            let handle = winapi::um::synchapi::CreateEventA(null_mut(), 0, 0, null());
            if handle.is_null() {
                panic!("CreateEventA failed. That should not happen...");
            }
            handle
        };
        PCan::register_event(handle, event_handle);
        log::debug!("Waiter Event registered");
        Waiter { event_handle }
    }

    pub(crate) fn wait_for_event(&self) {
        unsafe {
            let err = winapi::um::synchapi::WaitForSingleObject(self.event_handle, 100);
            if err == winapi::um::winbase::WAIT_FAILED {
                panic!("Waiting for event has failed!");
            }
        }
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.event_handle);
        }
    }
}
