mod api;
mod sys;
use std::thread;
use tokio::sync::mpsc;
use std::sync::mpsc;


struct PCanWriter {
    channel: api::Handle, 
}

impl PCanWriter {
    pub fn start(channel: api::Handle) -> mpsc::UnboundedSender<CanMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        let writer = PCanWriter {
            channel 
        }; 
        thread::spawn(|| writer.run());
        tx
    }

    pub fn run(self)  {

    }
}
