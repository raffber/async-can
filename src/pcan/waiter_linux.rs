// https://forum.peak-system.com/viewtopic.php?t=3817

use super::api::PCan;
use crate::pcan::api::Handle;
use std::{
    os::unix::prelude::RawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
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
            return Ok(self.cancel.load(Ordering::SeqCst));
        }
        if polls[0].revents != 0 {
            // this was an event for CAN file
            return Ok(false);
        }
        Ok(false)
    }
}
