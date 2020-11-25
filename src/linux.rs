use crate::Message;
use crate::Result;
use crate::socketcan::CanSocket;

pub struct Bus {
    socket: CanSocket,
}

impl Bus {
    fn connect(ifname: String) -> Result<Self> {
        let socket = CanSocket::bind(ifname)?;
        Ok(Bus {
            socket
        })
    }

    async fn send(&self, msg: Message) -> Result<()> {
        Ok(self.socket.send(msg).await?)
    }

    async fn recv(&self) -> Result<Message> {
        Ok(self.socket.recv().await?)
    }
}

