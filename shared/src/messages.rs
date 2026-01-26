#[repr(C, packed)]
pub struct Header {
    pub start: [u8; 4],
    pub length: [u8; 2],
}
