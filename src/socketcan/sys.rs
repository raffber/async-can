
const CAN_EFF_FLAG: u32 = 0x80000000;
const CAN_RTR_FLAG: u32 = 0x40000000;
const CAN_ERR_FLAG: u32 = 0x20000000;

const CAN_SFF_MASK: u32 = 0x7FF;
const CAN_EFF_MASK: u32 = 0x1FFFFFFF;
const CAN_ERR_MASK: u32 = 0x1FFFFFFF;

const CAN_SFF_ID_BITS: u32 = 11;
const CAN_EFF_ID_BITS: u32 = 29;

const CAN_MAX_DLC: usize = 8;
const CAN_MAX_DLEN: usize = 8;

#[repr(C)]
struct CanFrame {
    id: u32,
    dlc: u8,
    pad: u8,
    res0: u8,
    res1: u8,
    data: [u8; CAN_MAX_DLEN],
}

const CAN_RAW: usize = 1;