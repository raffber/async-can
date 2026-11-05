// https://forum.peak-system.com/viewtopic.php?t=3817

use libc::c_void;

use super::api::PCan;
use crate::pcan::api::Handle;
use std::{
    os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(crate) struct Waiter {
    handle: Handle,
    /// Owned by the PCAN driver.
    fd: RawFd,
    /// Shared with the [`WaiterHandle`], so it stays open until both are dropped.
    eventfd: Arc<OwnedFd>,
    cancel: Arc<AtomicBool>,
}

pub(crate) struct WaiterHandle {
    eventfd: Arc<OwnedFd>,
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
            return Err(crate::Error::Io(std::io::Error::last_os_error()));
        }
        // SAFETY: `eventfd` is a freshly created, valid fd that nothing else owns.
        let eventfd = Arc::new(unsafe { OwnedFd::from_raw_fd(eventfd) });

        let cancel = Arc::new(AtomicBool::new(false));

        let waiter = Self {
            handle,
            fd,
            eventfd: eventfd.clone(),
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
                fd: self.eventfd.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let err = unsafe {
            libc::poll(
                &mut polls as *mut libc::pollfd,
                polls.len() as libc::nfds_t,
                -1,
            )
        };
        if err < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                // Interrupted by a signal: let the caller check for messages and wait again.
                return Ok(self.cancel.load(Ordering::SeqCst));
            }
            return Err(crate::Error::Io(err));
        }
        if polls[1].revents != 0 {
            // eventfd was flagged
            let mut data = [0_u8; 8];
            unsafe {
                libc::read(
                    self.eventfd.as_raw_fd(),
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

impl WaiterHandle {
    pub(crate) fn close(&mut self) {
        // Writing adds the value to the eventfd counter, which wakes poll() only if it's non-zero.
        let data = 1_u64.to_ne_bytes();
        self.cancel.store(true, Ordering::SeqCst);
        unsafe {
            libc::write(
                self.eventfd.as_raw_fd(),
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

    #[test]
    fn eventfd_outlives_waiter() {
        let mut pipe = [0; 2];
        assert_eq!(unsafe { libc::pipe(pipe.as_mut_ptr()) }, 0);
        let (waiter, mut handle) = Waiter::with_fd(0, pipe[0]).unwrap();
        let eventfd = handle.eventfd.as_raw_fd();

        // The receive thread may exit, dropping the waiter, before the handle is closed.
        drop(waiter);
        // The eventfd must still be open, so close() cannot write to a reused fd number.
        assert_ne!(unsafe { libc::fcntl(eventfd, libc::F_GETFD) }, -1);
        handle.close();

        unsafe {
            libc::close(pipe[0]);
            libc::close(pipe[1]);
        }
    }
}
