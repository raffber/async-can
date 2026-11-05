use std::ptr::{null, null_mut};

use super::api::PCan;
use crate::pcan::api::Handle;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::WAIT_FAILED;
use windows_sys::Win32::System::Threading::{CreateEventA, SetEvent, WaitForSingleObject};

pub(crate) struct Waiter {
    /// Shared with the [`WaiterHandle`], so it stays open until both are dropped.
    event: Arc<OwnedHandle>,
    cancel: Arc<AtomicBool>,
}

pub(crate) struct WaiterHandle {
    event: Arc<OwnedHandle>,
    cancel: Arc<AtomicBool>,
}

impl Waiter {
    pub(crate) fn new(handle: Handle) -> crate::Result<(Self, WaiterHandle)> {
        let event = unsafe {
            let handle = CreateEventA(null_mut(), 0, 0, null());
            if handle.is_null() {
                return Err(crate::Error::Io(std::io::Error::last_os_error()));
            }
            // SAFETY: `handle` is a freshly created, valid event handle that nothing else owns.
            Arc::new(OwnedHandle::from_raw_handle(handle))
        };
        PCan::register_event(handle, event.as_raw_handle())
            .map_err(|err| crate::Error::PCanInitFailed(err.code, err.description()))?;
        log::debug!("Waiter Event registered");
        let cancel = Arc::new(AtomicBool::new(false));
        Ok((
            Waiter {
                event: event.clone(),
                cancel: cancel.clone(),
            },
            WaiterHandle { event, cancel },
        ))
    }

    pub(crate) fn wait_for_event(&self) -> crate::Result<bool> {
        let result = unsafe { WaitForSingleObject(self.event.as_raw_handle(), 100) };
        if result == WAIT_FAILED {
            return Err(crate::Error::Io(std::io::Error::last_os_error()));
        }
        Ok(self.cancel.load(Ordering::SeqCst))
    }
}

impl WaiterHandle {
    pub(crate) fn close(&mut self) {
        self.cancel.store(true, Ordering::SeqCst);
        unsafe {
            SetEvent(self.event.as_raw_handle());
        }
    }
}
