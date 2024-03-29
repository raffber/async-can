//! This module implements support for the USR-CANET200 protocol support for the respective devices from [USR-IOT](https://www.pusr.com/products/can-to-ethernet-converters-usr-canet200.html)
//!
//! The manual describing the protocol is [here](https://www.pusr.com/products/can-to-ethernet-converters-usr-canet200.html).
//! It's a very simple protocol for framing CAN messages on TCP without support for CAN-FD.

use crate::Message;
use async_trait::async_trait;
use byteorder::{BigEndian, ByteOrder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::ToSocketAddrs;
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
};

/// A sender for the USR-CANET200 device. Implements [`crate::Sender`].
///
/// Contains the write half of the TCP stream.
pub struct Sender {
    stream: OwnedWriteHalf,
}

/// A receiver for the USR-CANET200 device. Implements [`crate::Receiver`].
///
/// Contains the read half of the TCP stream.
pub struct Receiver {
    stream: OwnedReadHalf,
}

/// Construct a sender and receiver by connecting a TCP stream to the given device.
pub async fn connect<A: ToSocketAddrs>(addr: A) -> crate::Result<(Sender, Receiver)> {
    let stream = TcpStream::connect(addr).await?;
    stream.set_nodelay(true)?;
    let (read, write) = stream.into_split();
    let sender = Sender { stream: write };
    let receiver = Receiver { stream: read };
    Ok((sender, receiver))
}

#[async_trait]
impl crate::Sender for Sender {
    async fn send(&mut self, msg: Message) -> crate::Result<()> {
        let mut buf = [0_u8; 13];
        buf[0] = if msg.ext_id() { 0x80_u8 } else { 0x00 };
        buf[0] |= msg.dlc() & 0xF;
        BigEndian::write_u32(&mut buf[1..], msg.id());
        match msg {
            Message::Data(msg) => {
                buf[5..5 + msg.dlc() as usize].copy_from_slice(msg.data());
            }
            Message::Remote(msg) => {
                buf[0] |= 0x40;
                BigEndian::write_u32(&mut buf[1..], msg.id());
            }
        }
        self.stream.write_all(&buf).await?;
        Ok(())
    }
}

#[async_trait]
impl crate::Receiver for Receiver {
    async fn recv(&mut self) -> crate::Result<Message> {
        let mut buf = [0_u8; 13];
        self.stream.read_exact(&mut buf).await?;

        let ext_id = (buf[0] & 0x80) != 0;
        let id = BigEndian::read_u32(&buf[1..]);
        let dlc = buf[0] & 0xF;
        let ret = if (buf[0] & 0x40) != 0 {
            Message::new_remote(id, ext_id, dlc)?
        } else {
            Message::new_data(id, ext_id, &buf[5..5 + (dlc as usize)])?
        };
        Ok(ret)
    }
}

#[cfg(test)]
mod test {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task,
    };

    use crate::{Message, Receiver, Sender};

    #[tokio::test]
    async fn round_trip() {
        let listener = TcpListener::bind("127.0.0.1:1234").await.unwrap();
        task::spawn(async move {
            let (mut connection, _) = listener.accept().await.unwrap();
            while let Ok(x) = connection.read_u8().await {
                if connection.write_u8(x).await.is_err() {
                    break;
                }
            }
        });
        let (mut tx, mut rx) = super::connect("127.0.0.1:1234").await.unwrap();

        let tx_msg = Message::new_data(0xABCDEF, true, &[1, 2, 3, 4]).unwrap();
        tx.send(tx_msg.clone()).await.unwrap();
        let rx_msg = rx.recv().await.unwrap();
        assert_eq!(tx_msg, rx_msg);

        let tx_msg = Message::new_remote(0x123456, true, 3).unwrap();
        tx.send(tx_msg.clone()).await.unwrap();
        let rx_msg = rx.recv().await.unwrap();
        assert_eq!(tx_msg, rx_msg);
    }
}
