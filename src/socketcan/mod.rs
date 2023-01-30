//! Implements an async interface to the Linux SocketCAN

use std::ffi::{c_void, CString};
use std::io::{self, ErrorKind};
use std::mem::{size_of, MaybeUninit};
use std::os::raw::{c_int, c_short};
use std::os::unix::io::{AsRawFd, RawFd};
use std::task::{Context, Poll};

use futures::future::poll_fn;
use futures::{ready, TryStreamExt};
use libc;
use libc::sockaddr;
use mio::event::Source;
use mio::unix::SourceFd;
use rtnetlink::packet::nlas::link::{Info, InfoKind, Nla, State};
use tokio::io::unix::AsyncFd;

use crate::socketcan::sys::{CanFrame, CanSocketAddr, AF_CAN};
use crate::Message;
use crate::{DeviceInfo, Result};
use mio::{Interest, Registry, Token};

use async_trait::async_trait;

mod sys;

/// A type that connects to CAN socket
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
    /// Bind to the CAN socket with the given interface name
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

    /// Try to receive a [`crate::Message`] from the CAN bus
    async fn recv(&self) -> io::Result<Message> {
        poll_fn(|cx| self.poll_read(cx)).await
    }

    fn poll_read(&self, cx: &mut Context) -> Poll<io::Result<Message>> {
        loop {
            let mut guard = ready!(self.inner.poll_read_ready(cx))?;
            match guard.try_io(|fd| read_from_fd(fd.as_raw_fd())) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    /// Try to send a [`crate::Message`] to the CAN bus
    pub async fn send(&self, msg: Message) -> io::Result<()> {
        let frame: CanFrame = CanFrame::from(msg);
        let ret = poll_fn(|cx| self.poll_write(cx, &frame)).await;
        Ok(ret?)
    }

    fn poll_write(&self, cx: &mut Context<'_>, frame: &CanFrame) -> Poll<io::Result<()>> {
        loop {
            let mut guard = ready!(self.inner.poll_write_ready(cx))?;
            match guard.try_io(|fd| write_to_fd(fd.as_raw_fd(), frame)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        let new_fd = unsafe { libc::dup(self.as_raw_fd()) };
        if new_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            inner: AsyncFd::new(new_fd)?,
        })
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

#[async_trait]
impl crate::Sender for CanSocket {
    async fn send(&mut self, msg: Message) -> Result<()> {
        Ok(self.send(msg).await?)
    }
}

#[async_trait]
impl crate::Receiver for CanSocket {
    async fn recv(&mut self) -> Result<Message> {
        Ok(self.recv().await?)
    }
}

/// Return the index of the given interface
pub async fn get_interface_index_by_name(interface: &str) -> crate::Result<u32> {
    let devices = list_devices().await?;
    for device in &devices {
        if device.interface_name == interface {
            return Ok(device.index);
        }
    }
    Err(crate::Error::Io(io::Error::new(
        ErrorKind::NotFound,
        format!("Interface `{}` not found", interface),
    )))
}

/// Enable the given CAN interface.
///
/// This is like calling
/// ```sh
/// ip link set can0 up
/// ```
///
/// Note, that this requires the capability `CAP_NET_ADMIN`
pub async fn set_interface_up(interface: &str) -> crate::Result<()> {
    let index = get_interface_index_by_name(interface).await?;
    let (con, handle, _) = rtnetlink::new_connection()?;
    tokio::spawn(con);
    handle
        .link()
        .set(index)
        .up()
        .execute()
        .await
        .map_err(|x| crate::Error::Other(format!("{}", x)))
}

/// Disable the given CAN interface.
///
/// This is like calling
/// ```sh
/// ip link set can0 down
/// ```
///
/// Note, that this requires the capability `CAP_NET_ADMIN`
pub async fn set_interface_down(interface: &str) -> crate::Result<()> {
    let index = get_interface_index_by_name(interface).await?;
    let (con, handle, _) = rtnetlink::new_connection()?;
    tokio::spawn(con);
    handle
        .link()
        .set(index)
        .down()
        .execute()
        .await
        .map_err(|x| crate::Error::Other(format!("{}", x)))
}

/// List all SocketCAN interfaces
///
/// This is similar to using `ip link` but already filters for CAN interfaces
pub async fn list_devices() -> crate::Result<Vec<DeviceInfo>> {
    let (con, handle, _) = rtnetlink::new_connection()?;
    tokio::spawn(con);
    let mut links = handle.link().get().execute();
    let mut can_interfaces = Vec::new();
    while let Some(msg) = links
        .try_next()
        .await
        .map_err(|x| crate::Error::Other(format!("{}", x)))?
    {
        let mut info = DeviceInfo {
            interface_name: "".to_string(),
            is_ready: false,
            index: msg.header.index,
        };
        let mut is_can = false;
        for nla in msg.nlas.into_iter() {
            match nla {
                Nla::IfName(name) => {
                    info.interface_name = name;
                }
                Nla::Info(infos) => {
                    for info in infos {
                        if let Info::Kind(InfoKind::Other(kind)) = info {
                            if kind == "can" || kind == "vcan" {
                                is_can = true;
                            }
                        }
                    }
                }
                Nla::OperState(State::Up) | Nla::OperState(State::Unknown) => {
                    info.is_ready = true;
                }
                _ => {}
            }
        }
        if is_can {
            can_interfaces.push(info);
        }
    }
    Ok(can_interfaces)
}

#[cfg(test)]
mod test {
    use super::*;

    #[ignore]
    #[tokio::test]
    async fn socketcan_devices_up_down() {
        let devices = list_devices().await.unwrap();
        println!("{:?}", devices);
        devices
            .iter()
            .find(|x| x.interface_name == "vcan0")
            .expect("`vcan0` device not found.");
        set_interface_up("vcan0").await.unwrap();
        let devices = list_devices().await.unwrap();
        println!("{:?}", devices);
        devices
            .iter()
            .find(|x| x.interface_name == "vcan0" && x.is_ready)
            .expect("`vcan0` device not up.");
        set_interface_down("vcan0").await.unwrap();
        let devices = list_devices().await.unwrap();
        devices
            .iter()
            .find(|x| x.interface_name == "vcan0" && !x.is_ready)
            .expect("`vcan0` device not down.");
    }
}
