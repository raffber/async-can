//! This module implements "dummy" loopback deviec. This is mostly intended for testing.

use async_trait::async_trait;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

use crate::Message;

#[derive(Clone)]
pub struct Sender {
    tx: UnboundedSender<Message>,
}

pub struct Receiver {
    rx: UnboundedReceiver<Message>,
}

/// Create a connected [`Sender`] and [`Receiver`] using an MPSC channel.
pub fn connect() -> (Sender, Receiver) {
    let (tx, rx) = unbounded_channel();
    (Sender { tx }, Receiver { rx })
}

#[async_trait]
impl crate::Sender for Sender {
    async fn send(&mut self, msg: Message) -> crate::Result<()> {
        self.tx
            .send(msg)
            .map_err(|_| crate::Error::Other(format!("Disconnected")))
    }
}

#[async_trait]
impl crate::Receiver for Receiver {
    async fn recv(&mut self) -> crate::Result<Message> {
        self.rx
            .recv()
            .await
            .ok_or_else(|| crate::Error::Other(format!("Disconnected")))
    }
}
