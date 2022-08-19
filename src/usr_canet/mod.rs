use crate::Message;
use async_trait::async_trait;
use byteorder::{BigEndian, ByteOrder};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::ToSocketAddrs;
use tokio::net::{
    tcp::{OwnedReadHalf, OwnedWriteHalf},
    TcpStream,
};

pub struct Sender {
    stream: OwnedWriteHalf,
}

pub struct Receiver {
    stream: OwnedReadHalf,
}

async fn connect<A: ToSocketAddrs>(addr: A) -> crate::Result<(Sender, Receiver)> {
    let stream = TcpStream::connect(addr).await?;
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
                buf[5..].copy_from_slice(msg.data());
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
