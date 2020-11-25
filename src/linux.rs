use crate::Message;
use std::io;

pub struct Listener {

}

impl Listener {
    pub async fn recv(&self) -> io::Result<Message> {
        todo!()
    }
}

pub struct Sender {

}

impl Sender {
    pub async fn send(&self, msg: &Message) -> io::Result<()> {
        todo!()
    }
}
