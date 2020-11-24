mod sys;

use std::os::unix::io::{AsRawFd, RawFd};
use std::io;
use crate::CanMessage;
use libc;
use libc::sockaddr;
use std::ffi::{CString, c_void};
use std::os::raw::{c_int, c_short};
use crate::socketcan::sys::{SocketAddr, AF_CAN, CanFrame};
use std::mem::size_of;
use mio::event::Evented;
use mio::unix::EventedFd;
use mio::{Ready, PollOpt, Token};
use std::task::Poll;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use futures::ready;
use tokio::io::PollEvented;

struct EventedSocket(RawFd);

pub struct CanSocket {
    inner: PollEvented<EventedSocket>,
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

        let addr = SocketAddr {
            _af_can: AF_CAN as c_short,
            if_index: ifindex as c_int,
            rx_id: 0,
            tx_id: 0,
        };
        let ok = unsafe {
            libc::bind(fd, &addr as *const SocketAddr as *const sockaddr, size_of::<SocketAddr>() as u32)
        };
        if ok != 0 {
            return Err(io::Error::last_os_error());
        }
        let inner = PollEvented::new(EventedSocket(fd)).expect("No tokio runtime");
        Ok(Self {
            inner
        })
    }

    async fn recv(&self) -> io::Result<CanMessage> {
        todo!()
    }

    fn send(&self, msg: &CanMessage) -> Write {
        todo!()
    }
}

pub struct Write {
    socket: CanSocket,
    frame: CanFrame,
}

impl Future for Write {
    type Output = io::Result<()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        ready!(self.socket.inner.poll_write_ready(cx))?;

        let written = unsafe {
            libc::write(self.socket.inner, &self.frame as *const CanFrame as *const c_void, size_of::<CanFrame>())
        };
        if written as usize != size_of::<CanFrame>() {
            Poll::Ready(Err(io::Error::last_os_error()))
        } else {
            Poll::Ready(Ok(()))
        }
    }
}

impl AsRawFd for CanSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner
    }
}

impl Evented for CanSocket {
    fn register(&self, poll: &mio::Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.inner).register(poll, token, interest, opts)
    }

    fn reregister(&self, poll: &mio::Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
        EventedFd(&self.inner).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.inner).deregister(poll)
    }
}