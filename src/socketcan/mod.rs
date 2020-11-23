mod sys;

use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::io;
use crate::can::CanMessage;
use crate::CanMessage;
use libc;
use std::ffi::CString;
use std::os::raw::c_int;

pub struct CanSocket {
    inner: RawFd,
}

impl CanSocket {
    pub fn bind<T: AsRef<str>>(ifname: T) -> io::Result<Self> {
        let name = CString::new(ifname.as_ref()).unwrap();
        let ifindex = unsafe {
            libc::if_nametoindex(name.as_ptr())
        };
        if ifindex == 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe {
            libc::socket(libc::PF_CAN, libc::SOCK_RAW, sys::CAN_RAW as c_int)
        };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }


        let ok = unsafe {
            // libc::bind(fd, )

        }

        todo!()
    }

    async fn recv(&self) -> io::Result<CanMessage> {
        todo!()
    }

    async fn send(&self, msg: &CanMessage) -> io::Result<()> {
        todo!()
    }
}

impl AsRawFd for CanSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner
    }
}
