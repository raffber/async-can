use std::ffi::{c_void, CString};
use std::io;
use std::mem::{size_of, MaybeUninit};
use std::os::raw::{c_int, c_short};
use std::os::unix::io::{AsRawFd, RawFd};
use std::sync::Arc;
use std::task::{Poll, Context};

use futures::future::poll_fn;
use futures::ready;
use libc;
use libc::sockaddr;
use mio::event::Source;
use mio::unix::SourceFd;
use tokio::io::unix::AsyncFd;

use crate::socketcan::sys::{CanFrame, CanSocketAddr, AF_CAN};
use crate::Result;
use crate::{Message, Timestamp};
use mio::{Interest, Registry, Token};

mod sys;

pub struct CanSocket {
    inner: AsyncFd<RawFd>,
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

        let inner = AsyncFd::new(fd)?;
        Ok(Self { inner })
    }

    pub async fn recv(&self) -> io::Result<Message> {
        poll_fn(|cx|  self.poll_read(cx) ).await
    }

    fn poll_read(&self, cx: &mut Context) -> Poll<io::Result<Message>> {
        let mut guard = ready!(self.inner.poll_read_ready(cx))?;
        match guard.try_io(|fd| read_from_fd(fd.as_raw_fd())) {
            Ok(result) => return Poll::Ready(result),
            Err(_would_block) => (),
        };
        let mut guard = ready!(self.inner.poll_read_ready(cx))?;
        match guard.try_io(|fd| read_from_fd(fd.as_raw_fd())) {
            Ok(result) => Poll::Ready(result),
            Err(_would_block) => Poll::Pending,
        }
    }

    pub async fn send(&self, msg: Message) -> crate::Result<()> {
        let frame: CanFrame = CanFrame::from_message(msg)?;
        let ret = poll_fn(|cx| self.poll_write(cx, &frame)).await;
        Ok(ret?)
    }

    fn poll_write(&self, cx: &mut Context<'_>, frame: &CanFrame) -> Poll<io::Result<()>> {
        let mut guard = ready!(self.inner.poll_write_ready(cx))?;
        match guard.try_io(|fd| write_to_fd(fd.as_raw_fd(), frame)) {
            Ok(result) => return Poll::Ready(result),
            Err(_would_block) => (),
        }
        let mut guard = ready!(self.inner.poll_write_ready(cx))?;
        match guard.try_io(|fd| write_to_fd(fd.as_raw_fd(), frame)) {
            Ok(result) => Poll::Ready(result),
            Err(_would_block) => Poll::Pending,
        }
    }
}

fn write_to_fd(fd: RawFd, frame: &CanFrame) -> io::Result<()> {
    let frame = frame as *const CanFrame as *const c_void;
    let written = unsafe { libc::write(fd, frame, size_of::<CanFrame>()) };
    if written as usize != size_of::<CanFrame>() {
        Err(io::Error::last_os_error())
    } else {
        // successfully sent
        Ok(())
    }

}

fn read_from_fd(fd: RawFd) -> io::Result<Message> {
    let mut frame = MaybeUninit::<CanFrame>::uninit();
    let (frame, size) = unsafe {
        let size = libc::read(fd, frame.as_mut_ptr() as *mut c_void, size_of::<CanFrame>());
        (frame.assume_init(), size as usize)
    };
    if size != size_of::<CanFrame>() {
        return Err(io::Error::last_os_error());
    }
    Ok(frame.into())
}

impl Source for CanSocket {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        SourceFd(&self.as_raw_fd()).deregister(registry)
    }
}

#[derive(Clone)]
pub struct Sender {
    socket: Arc<CanSocket>,
}

pub struct Receiver {
    socket: CanSocket,
}

impl Sender {
    pub fn connect(ifname: String) -> Result<Self> {
        let socket = CanSocket::bind(ifname)?;
        Ok(Sender {
            socket: Arc::new(socket),
        })
    }

    pub async fn send(&self, msg: Message) -> Result<()> {
        Ok(self.socket.send(msg).await?)
    }
}

impl Receiver {
    pub fn connect(ifname: String) -> Result<Self> {
        let socket = CanSocket::bind(ifname)?;
        Ok(Receiver { socket })
    }

    pub async fn recv(&self) -> Result<Message> {
        Ok(self.socket.recv().await?)
    }

    pub async fn recv_with_timestamp(&self) -> Result<(Message, Timestamp)> {
        todo!()
    }
}
