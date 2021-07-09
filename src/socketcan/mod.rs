use std::ffi::{c_void, CString};
use std::io;
use std::mem::{MaybeUninit, size_of};
use std::os::raw::{c_int, c_short};
use std::os::unix::io::{AsRawFd, RawFd};
use std::task::Poll;

use futures::future::poll_fn;
use futures::ready;
use libc;
use libc::sockaddr;
use mio::{PollOpt, Ready, Token};
use mio::event::Evented;
use mio::unix::{EventedFd, UnixReady};
use tokio::io::{ErrorKind, PollEvented};

use crate::{Message, Timestamp};
use crate::socketcan::sys::{AF_CAN, CanFrame, CanSocketAddr};

mod sys;

impl AsRawFd for EventedSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.0
    }
}

struct EventedSocket(RawFd);

pub struct CanSocket {
    inner: PollEvented<EventedSocket>,
}

impl Drop for CanSocket {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.as_raw_fd());
        }
    }
}

impl AsRawFd for CanSocket {
    fn as_raw_fd(&self) -> RawFd {
        self.inner.get_ref().as_raw_fd()
    }
}

impl CanSocket {
    pub fn bind<T: AsRef<str>>(ifname: T) -> io::Result<Self> {
        let name = CString::new(ifname.as_ref()).unwrap();
        let ifindex = unsafe { libc::if_nametoindex(name.as_ptr()) };
        if ifindex == 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { libc::socket(libc::PF_CAN, libc::SOCK_RAW, sys::CAN_RAW as c_int) };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }

        let addr = CanSocketAddr {
            _af_can: AF_CAN as c_short,
            if_index: ifindex as c_int,
            rx_id: 0,
            tx_id: 0,
        };
        let ok = unsafe {
            libc::bind(
                fd,
                &addr as *const CanSocketAddr as *const sockaddr,
                size_of::<CanSocketAddr>() as u32,
            )
        };
        if ok != 0 {
            return Err(io::Error::last_os_error());
        }

        // set non-blocking mode for asyncio
        let nonblocking = true;
        let ok = unsafe { libc::ioctl(fd, libc::FIONBIO, &(nonblocking as c_int)) };
        if ok != 0 {
            return Err(io::Error::last_os_error());
        }

        let inner = PollEvented::new(EventedSocket(fd))?;
        Ok(Self { inner })
    }

    fn read_from_fd(&self) -> io::Result<Message> {
        let mut frame = MaybeUninit::<CanFrame>::uninit();
        let (frame, size) = unsafe {
            let size = libc::read(
                self.as_raw_fd(),
                frame.as_mut_ptr() as *mut c_void,
                size_of::<CanFrame>(),
            );
            (frame.assume_init(), size as usize)
        };
        if size != size_of::<CanFrame>() {
            return Err(io::Error::last_os_error());
        }
        Ok(frame.into())
    }

    pub async fn recv(&self) -> io::Result<Message> {
        let ready = Ready::readable() | Ready::from(UnixReady::error());
        poll_fn(|cx| {
            ready!(self.inner.poll_read_ready(cx, ready))?;
            match self.read_from_fd() {
                Err(e) => {
                    if e.kind() == ErrorKind::WouldBlock {
                        self.inner.clear_read_ready(cx, ready)?;
                        Poll::Pending
                    } else {
                        Poll::Ready(Err(e))
                    }
                }
                Ok(ret) => Poll::Ready(Ok(ret)),
            }
        }).await
    }

    pub async fn recv_with_timestamp(&self) -> io::Result<(Message, Timestamp)> {
        todo!()
    }

    pub async fn send(&self, msg: Message) -> crate::Result<()> {
        let frame: CanFrame = CanFrame::from_message(msg)?;
        let ret = poll_fn(|cx| {
            ready!(self.inner.poll_write_ready(cx))?;
            let frame = &frame as *const CanFrame as *const c_void;
            let written = unsafe { libc::write(self.as_raw_fd(), frame, size_of::<CanFrame>()) };
            if written as usize != size_of::<CanFrame>() {
                let err = io::Error::last_os_error();
                if err.kind() == ErrorKind::WouldBlock {
                    // would block so not yet ready
                    self.inner.clear_write_ready(cx)?;
                    Poll::Pending
                } else if let Some(105) = err.raw_os_error() {
                    self.inner.clear_write_ready(cx)?;
                    Poll::Pending
                } else {
                    // an actual error
                    Poll::Ready(Err(err))
                }
            } else {
                // successfully sent
                Poll::Ready(Ok(()))
            }
        }).await;
        Ok(ret?)
    }
}

impl Evented for EventedSocket {
    fn register(
        &self,
        poll: &mio::Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.0).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        EventedFd(&self.0).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        EventedFd(&self.0).deregister(poll)
    }
}
