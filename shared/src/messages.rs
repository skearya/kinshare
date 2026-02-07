#[repr(C)]
pub struct Header {
    pub frame: [u8; 4],
    pub chunks: [u8; 4],
    pub x: [u8; 1],
    pub y: [u8; 1],
    pub size: [u8; 4],
    pub offset: [u8; 4],
}
