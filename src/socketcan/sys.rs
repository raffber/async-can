//! Conversions between [`Message`] and the kernel's `struct can_frame`, as defined by `libc`.

use std::mem;

use libc::{can_frame, CAN_EFF_FLAG, CAN_EFF_MASK, CAN_RTR_FLAG, CAN_SFF_MASK};

use crate::Message;

/// Builds a zero-initialized `can_frame`.
fn empty_frame() -> can_frame {
    // SAFETY: `can_frame` only contains integers and byte arrays, for which all-zeroes is valid.
    unsafe { mem::zeroed() }
}

impl From<Message> for can_frame {
    fn from(msg: Message) -> Self {
        let mut frame = empty_frame();
        frame.can_id = msg.id();
        if msg.ext_id() {
            frame.can_id |= CAN_EFF_FLAG;
        }
        match msg {
            Message::Data(msg) => {
                let data = msg.data();
                frame.data[..data.len()].copy_from_slice(data);
                frame.can_dlc = data.len() as u8;
            }
            Message::Remote(msg) => {
                frame.can_id |= CAN_RTR_FLAG;
                frame.can_dlc = msg.dlc();
            }
        }
        frame
    }
}

/// Converts a frame received from the kernel into a [`Message`].
pub(crate) fn frame_to_message(frame: &can_frame) -> Message {
    let (id, ext_id) = if frame.can_id & CAN_EFF_FLAG != 0 {
        (frame.can_id & CAN_EFF_MASK, true)
    } else {
        (frame.can_id & CAN_SFF_MASK, false)
    };
    if frame.can_id & CAN_RTR_FLAG != 0 {
        Message::new_remote(id, ext_id, frame.can_dlc).unwrap()
    } else {
        Message::new_data(id, ext_id, &frame.data[..frame.can_dlc as usize]).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn round_trip() {
        let msgs = [
            Message::new_data(0x123, false, &[1, 2, 3]).unwrap(),
            Message::new_data(0x1ABCDEF, true, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
            Message::new_data(0x7FF, false, &[]).unwrap(),
            Message::new_remote(0x42, false, 4).unwrap(),
            Message::new_remote(0x1FFFFFFF, true, 8).unwrap(),
        ];
        for msg in msgs {
            let frame = can_frame::from(msg.clone());
            assert_eq!(frame_to_message(&frame), msg);
        }
    }

    #[test]
    fn frame_layout() {
        let frame = can_frame::from(Message::new_remote(0x10, true, 2).unwrap());
        assert_eq!(frame.can_id, 0x10 | CAN_EFF_FLAG | CAN_RTR_FLAG);
        assert_eq!(frame.can_dlc, 2);
    }
}
