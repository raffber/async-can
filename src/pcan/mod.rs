mod api;
mod sys;
use std::thread;
use crate::{Result, Error};
use crate::{Message, Timestamp};

struct PCanDevice {

}

impl PCanDevice {
    fn connect(ifname: String) -> Result<Self> {
        todo!()
    }

    async fn send(&self, msg: Message) -> Result<()> {
        todo!()
    }

    async fn recv(&self) -> Result<Message> {
        todo!()
    }

    async fn recv_with_timestamp(&self) -> Result<(Message, Timestamp)> {
        todo!()
    }
}

