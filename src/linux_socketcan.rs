use crate::socketcan::CanSocket;
use crate::Result;
use crate::{Message, Timestamp};
use std::sync::Arc;


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
        Ok(self.socket.recv_with_timestamp().await?)
    }
}
