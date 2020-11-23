mod sys;

use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::io;
use crate::can::CanMessage;
use crate::CanMessage;

pub struct CanSocket {
    inner: RawFd,
}

impl CanSocket {
    pub fn bind<T>(ifname: T) -> io::Result<Self> {
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
