use std::hash::Hasher;
use std::ops::Range;
use std::{io, thread};

use rustc_hash::FxHasher;

use crate::consts::{
    CHUNK_HEIGHT, CHUNK_SIZE, CHUNK_WIDTH, DISPLAY_SIZE, DISPLAY_WIDTH, NUM_CHUNKS, NUM_THREADS,
};

#[derive(Clone)]
pub struct Chunk {
    pub x: u8,
    pub y: u8,
    pub hash: u64,
    pub size: usize,
    pub encoded: Vec<u8>,
}

impl Chunk {
    pub fn new(x: u8, y: u8) -> Self {
        Self {
            x,
            y,
            hash: 0,
            size: 0,
            encoded: vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)],
        }
    }

    pub fn encode(&mut self, framebuffer: &[u8], offset: usize, buffer: &mut [u8]) -> bool {
        let frame_top_left_x = self.x as usize * CHUNK_WIDTH;
        let frame_top_left_y = self.y as usize * CHUNK_HEIGHT;

        let mut hasher = FxHasher::default();

        for row in 0..CHUNK_HEIGHT {
            let frame_start =
                (frame_top_left_x + (frame_top_left_y + row) * DISPLAY_WIDTH) - offset;

            hasher.write(&framebuffer[frame_start..frame_start + CHUNK_WIDTH]);
        }

        let hash = hasher.finish();

        if self.hash == hash {
            return false;
        }

        self.hash = hash;

        for row in 0..CHUNK_HEIGHT {
            let frame_start =
                (frame_top_left_x + (frame_top_left_y + row) * DISPLAY_WIDTH) - offset;
            let buffer_start = row * CHUNK_WIDTH;

            buffer[buffer_start..buffer_start + CHUNK_WIDTH]
                .copy_from_slice(&framebuffer[frame_start..frame_start + CHUNK_WIDTH]);
        }

        self.size = lz4_flex::block::compress_into(buffer, &mut self.encoded)
            .expect("compression shouldn't fail");

        true
    }

    pub fn decode(framebuffer: &mut [u8], buffer: &[u8], x: u8, y: u8) {
        let frame_top_left_x = x as usize * CHUNK_WIDTH;
        let frame_top_left_y = y as usize * CHUNK_HEIGHT;

        for row in 0..CHUNK_HEIGHT {
            let frame_start = frame_top_left_x + (frame_top_left_y + row) * DISPLAY_WIDTH;
            let buffer_start = row * CHUNK_WIDTH;

            framebuffer[frame_start..frame_start + CHUNK_WIDTH]
                .copy_from_slice(&buffer[buffer_start..buffer_start + CHUNK_WIDTH]);
        }
    }
}

pub fn encode(file: i32, framebuffer: &mut [u8], chunks: &mut [Chunk]) -> [bool; NUM_CHUNKS] {
    let mut updated = [false; NUM_CHUNKS];

    thread::scope(|s| {
        for (n, ((framebuffer, chunks), updated)) in framebuffer
            .chunks_exact_mut(DISPLAY_SIZE / NUM_THREADS)
            .zip(chunks.chunks_exact_mut(NUM_CHUNKS / NUM_THREADS))
            .zip(updated.chunks_exact_mut(NUM_CHUNKS / NUM_THREADS))
            .enumerate()
        {
            s.spawn(move || {
                let offset = n * (DISPLAY_SIZE / NUM_THREADS);

                if unsafe {
                    libc::pread(
                        file,
                        framebuffer.as_mut_ptr().cast(),
                        DISPLAY_SIZE / NUM_THREADS,
                        offset as i64,
                    )
                } == -1
                {
                    panic!("pread error: {:?}", io::Error::last_os_error());
                }

                let mut buffer = [0; CHUNK_SIZE];

                for (chunk, updated) in chunks.into_iter().zip(updated.into_iter()) {
                    *updated = chunk.encode(framebuffer, offset, &mut buffer);
                }
            });
        }
    });

    updated
}

pub fn decode2(framebuffer: &mut [u8], decoded: &mut [u8], x: u8, y: u8, data: &[u8]) {
    lz4_flex::block::decompress_into(data, decoded).expect("decompression shouldn't fail");
}

pub fn decode(framebuffer: &mut [u8], decoded: &mut [u8], x: u8, y: u8, data: &[u8]) {
    lz4_flex::block::decompress_into(data, decoded).expect("decompression shouldn't fail");

    Chunk::decode(framebuffer, decoded, x, y);
}

pub fn framebuffer_indices(x: u8, y: u8) -> Range<usize> {
    let frame_top_left_x = x as usize * CHUNK_WIDTH;
    let frame_top_left_y = y as usize * CHUNK_HEIGHT;

    let frame_start = frame_top_left_x + frame_top_left_y * DISPLAY_WIDTH;
    let frame_end = frame_top_left_x + (frame_top_left_y + CHUNK_HEIGHT) * DISPLAY_WIDTH;

    frame_start..frame_end + CHUNK_WIDTH
}
