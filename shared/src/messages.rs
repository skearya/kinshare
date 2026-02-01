#[repr(C)]
pub struct Header {
    pub frame: [u8; 4],
    pub offset: [u8; 4],
    pub total: [u8; 4],
    pub x: [u8; 1],
    pub y: [u8; 1],
}
