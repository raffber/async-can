use crate::{Message, Timestamp};
use crate::Result;
use crate::socketcan::CanSocket;

#[derive(Clone)]
pub struct Bus {
    socket: PCanDevice,
}

impl Bus {
    pub fn connect(ifname: &str) -> Result<Self> {
        let socket = CanSocket::bind(ifname)?;
        Ok(Bus {
            socket
        })
    }

    pub async fn send(&self, msg: Message) -> Result<()> {
        Ok(self.socket.send(msg).await?)
    }

    pub async fn recv(&self) -> Result<Message> {
        Ok(self.socket.recv().await?)
    }

    pub async fn recv_with_timestamp(&self) -> Result<(Message, Timestamp)> {
        Ok(self.socket.recv_with_timestamp().await?)
    }
}
