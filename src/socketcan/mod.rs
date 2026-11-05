//! Implements an async interface to the Linux SocketCAN

use std::ffi::{c_void, CString};
use std::io::{self, ErrorKind};
use std::mem::{size_of, MaybeUninit};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::task::{Context, Poll};

use std::future::poll_fn;
use std::task::ready;

use futures::TryStreamExt;
use libc::{can_frame, sockaddr, sockaddr_can};
use rtnetlink::packet_route::link::{InfoKind, LinkAttribute, LinkInfo, State};
use rtnetlink::LinkUnspec;
use tokio::io::unix::AsyncFd;

use crate::Message;
use crate::{DeviceInfo, Result};

use async_trait::async_trait;

mod sys;

/// A type that connects to CAN socket
pub struct CanSocket {
    inner: AsyncFd<OwnedFd>,
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
        let fd = unsafe { libc::socket(libc::PF_CAN, libc::SOCK_RAW, libc::CAN_RAW) };
        if fd == -1 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: `fd` is a freshly created, valid socket that nothing else owns.
        // Wrapping it right away ensures it is closed on every error path below.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        // SAFETY: `sockaddr_can` is plain old data, for which all-zeroes is valid.
        let mut addr: sockaddr_can = unsafe { std::mem::zeroed() };
        addr.can_family = libc::AF_CAN as libc::sa_family_t;
        addr.can_ifindex = ifindex as libc::c_int;
        let ok = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                &addr as *const sockaddr_can as *const sockaddr,
                size_of::<sockaddr_can>() as libc::socklen_t,
            )
        };
        if ok != 0 {
            return Err(io::Error::last_os_error());
        }

        // set non-blocking mode for asyncio
        let nonblocking = true;
        let ok =
            unsafe { libc::ioctl(fd.as_raw_fd(), libc::FIONBIO, &(nonblocking as libc::c_int)) };
        if ok != 0 {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: `OwnedFd` owns the descriptor and closes it only when the `AsyncFd` is dropped.
        let inner = unsafe { AsyncFd::register(fd)? };
        Ok(Self { inner })
    }

    /// Try to receive a [`crate::Message`] from the CAN bus
    pub async fn recv(&self) -> io::Result<Message> {
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
        let frame = can_frame::from(msg);
        let ret = poll_fn(|cx| self.poll_write(cx, &frame)).await;
        Ok(ret?)
    }

    fn poll_write(&self, cx: &mut Context<'_>, frame: &can_frame) -> Poll<io::Result<()>> {
        loop {
            let mut guard = ready!(self.inner.poll_write_ready(cx))?;
            match guard.try_io(|fd| write_to_fd(fd.as_raw_fd(), frame)) {
                Ok(result) => return Poll::Ready(result),
                Err(_would_block) => continue,
            }
        }
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        let new_fd = self.inner.get_ref().try_clone()?;
        // SAFETY: `OwnedFd` owns the duplicated descriptor and closes it only when the
        // `AsyncFd` is dropped.
        let inner = unsafe { AsyncFd::register(new_fd)? };
        Ok(Self { inner })
    }
}

fn write_to_fd(fd: RawFd, frame: &can_frame) -> io::Result<()> {
    let frame = frame as *const can_frame as *const c_void;
    let written = unsafe { libc::write(fd, frame, size_of::<can_frame>()) };
    if written as usize != size_of::<can_frame>() {
        Err(io::Error::last_os_error())
    } else {
        // successfully sent
        Ok(())
    }
}

fn read_from_fd(fd: RawFd) -> io::Result<Message> {
    let mut frame = MaybeUninit::<can_frame>::uninit();
    let size = unsafe {
        libc::read(
            fd,
            frame.as_mut_ptr() as *mut c_void,
            size_of::<can_frame>(),
        )
    };
    if size < 0 || size as usize != size_of::<can_frame>() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: the kernel wrote a complete `can_frame`.
    let frame = unsafe { frame.assume_init() };
    Ok(sys::frame_to_message(&frame))
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
        .set(LinkUnspec::new_with_index(index).up().build())
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
        .set(LinkUnspec::new_with_index(index).down().build())
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
        for attr in msg.attributes.into_iter() {
            match attr {
                LinkAttribute::IfName(name) => {
                    info.interface_name = name;
                }
                LinkAttribute::LinkInfo(infos) => {
                    for info in infos {
                        if let LinkInfo::Kind(InfoKind::Can | InfoKind::Vcan) = info {
                            is_can = true;
                        }
                    }
                }
                LinkAttribute::OperState(State::Up) | LinkAttribute::OperState(State::Unknown) => {
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
