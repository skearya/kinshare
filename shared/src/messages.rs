#[repr(C)]
pub struct Header {
    pub frame: [u8; 4],
    pub size: [u8; 4],
    pub offset: [u8; 4],
    pub length: [u8; 2],
}
