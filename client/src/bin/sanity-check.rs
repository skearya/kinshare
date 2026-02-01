use std::array;
use std::fs::File;
use std::os::fd::AsRawFd;

use shared::codec;
use shared::codec::Chunk;
use shared::consts::{
    CHUNK_HEIGHT, CHUNK_SIZE, CHUNK_WIDTH, DISPLAY_HEIGHT, DISPLAY_SIZE, DISPLAY_WIDTH, NUM_CHUNKS,
    PACKET_SIZE,
};
use shared::messages::Header;

fn main() -> anyhow::Result<()> {
    let fb0 = File::open("raw/frame.raw")?;
    let mut framebuffer = vec![0; DISPLAY_SIZE];
    let mut chunks: [Chunk; NUM_CHUNKS] = array::from_fn(|i| Chunk {
        x: (i % (DISPLAY_WIDTH / CHUNK_WIDTH)) as u8,
        y: (i / (DISPLAY_HEIGHT / CHUNK_HEIGHT)) as u8,
        encoded: vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)],
        hash: 0,
        size: 0,
    });

    let changed = codec::encode(fb0.as_raw_fd(), &mut framebuffer, &mut chunks);
    assert_eq!(changed, [true; 64]);

    let mut framebuffer2 = vec![0; DISPLAY_SIZE];
    let mut decoded = vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)];

    for chunk in &mut chunks {
        codec::decode(
            &mut framebuffer2,
            &mut decoded,
            chunk.x,
            chunk.y,
            &chunk.encoded[..chunk.size],
        );
    }

    dbg!(framebuffer == framebuffer2);

    Ok(())
}
