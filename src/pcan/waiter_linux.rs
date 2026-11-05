// https://forum.peak-system.com/viewtopic.php?t=3817

use libc::c_void;

use super::api::PCan;
use crate::pcan::api::Handle;
use std::{
    os::unix::prelude::RawFd,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) struct Waiter {
    handle: Handle,
    fd: RawFd,
    eventfd: RawFd,
    cancel: Arc<AtomicBool>,
}

pub(crate) struct WaiterHandle {
    eventfd: RawFd,
    cancel: Arc<AtomicBool>,
}

impl Waiter {
    pub(crate) fn new(handle: Handle) -> crate::Result<(Self, WaiterHandle)> {
        let fd = PCan::get_fd(handle)
            .map_err(|x| crate::Error::PCanInitFailed(x.code, x.description()))?;
        Self::with_fd(handle, fd)
    }

    /// Creates a waiter that waits for `fd` to become readable.
    fn with_fd(handle: Handle, fd: RawFd) -> crate::Result<(Self, WaiterHandle)> {
        let eventfd = unsafe { libc::eventfd(0, 0) };
        if eventfd == -1 {
            return Err(crate::Error::Io(std::io::Error::last_os_error()))?;
        }

        let cancel = Arc::new(AtomicBool::new(false));

        let waiter = Self {
            handle,
            fd,
            eventfd,
            cancel: cancel.clone(),
        };

        let waiter_handle = WaiterHandle { eventfd, cancel };
        Ok((waiter, waiter_handle))
    }

    pub(crate) fn wait_for_event(&self) -> crate::Result<bool> {
        let mut polls = [
            libc::pollfd {
                fd: self.fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: self.eventfd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let err = unsafe { libc::poll(&mut polls as *mut libc::pollfd, polls.len() as u64, -1) };
        if err < 0 {
            return Err(crate::Error::Io(std::io::Error::last_os_error()))?;
        }
        if polls[1].revents != 0 {
            // eventfd was flagged
            let mut data = [0_u8; 8];
            unsafe {
                libc::read(
                    self.eventfd,
                    &mut data as *mut u8 as *mut c_void,
                    data.len(),
                );
            }
        }
        if polls[0].revents != 0 {
            // this was an event for CAN file
        }
        Ok(self.cancel.load(Ordering::SeqCst))
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.eventfd);
        }
    }
}

impl WaiterHandle {
    pub(crate) fn close(&mut self) {
        // Writing adds the value to the eventfd counter, which wakes poll() only if it's non-zero.
        let data = 1_u64.to_ne_bytes();
        self.cancel.store(true, Ordering::SeqCst);
        unsafe {
            libc::write(
                self.eventfd,
                &data as *const u8 as *const c_void,
                data.len(),
            );
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{sync::mpsc, thread, time::Duration};

    #[test]
    fn close_wakes_waiter() {
        // A pipe that never becomes readable stands in for the PCAN driver's fd.
        let mut pipe = [0; 2];
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        let (waiter, mut handle) = Waiter::with_fd(0, pipe[0]).unwrap();

        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(waiter.wait_for_event().unwrap());
        });
        // Give the thread time to block in poll().
        thread::sleep(Duration::from_millis(50));
        handle.close();

        let cancelled = rx
            .recv_timeout(Duration::from_secs(1))
            .expect("close() did not wake the waiter");
        assert!(cancelled);
        unsafe {
            libc::close(pipe[0]);
            libc::close(pipe[1]);
        }
    }
}
