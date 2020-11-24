use crate::CanMessage;
use std::io;

pub struct Listener {

}

impl Listener {
    pub async fn recv(&self) -> io::Result<CanMessage> {
        todo!()
    }
}

pub struct Sender {

}

impl Sender {
    pub async fn send(&self, msg: &CanMessage) -> io::Result<()> {
        todo!()
    }
}
