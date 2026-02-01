pub const THREADS: usize = 2;

pub const DISPLAY_WIDTH: usize = 1872;
pub const DISPLAY_HEIGHT: usize = 2480;
pub const DISPLAY_SIZE: usize = DISPLAY_WIDTH * DISPLAY_HEIGHT;

pub const CHUNK_WIDTH: usize = 234;
pub const CHUNK_HEIGHT: usize = 310;
pub const CHUNK_SIZE: usize = CHUNK_WIDTH * CHUNK_HEIGHT;
pub const NUM_CHUNKS: usize = DISPLAY_SIZE / CHUNK_SIZE;

pub const PACKET_SIZE: usize = 1472;
