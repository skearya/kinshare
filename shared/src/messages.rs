use std::{
    hash::Hasher,
    sync::{Arc, Mutex},
};

use iroh::endpoint::{RecvStream, SendStream};
use rustc_hash::FxHasher;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Info {
    pub display_width: usize,
    pub display_height: usize,
    pub chunks_per_x: usize,
    pub chunks_per_y: usize,
    pub thread_count: usize,
    pub fps: f64,
}

impl Info {
    pub fn display_size(&self) -> usize {
        self.display_width * self.display_height
    }

    pub fn chunk_width(&self) -> usize {
        self.display_width / self.chunks_per_x
    }

    pub fn chunk_height(&self) -> usize {
        self.display_height / self.chunks_per_y
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks_per_x * self.chunks_per_y
    }

    pub fn chunk_size(&self) -> usize {
        self.chunk_width() * self.chunk_height()
    }
}

pub async fn write_info(stream: &mut SendStream, info: &Info) -> anyhow::Result<()> {
    stream.write_u64(info.display_width as u64).await?;
    stream.write_u64(info.display_height as u64).await?;
    stream.write_u64(info.chunks_per_x as u64).await?;
    stream.write_u64(info.chunks_per_y as u64).await?;
    stream.write_u64(info.thread_count as u64).await?;
    stream.write_f64(info.fps).await?;

    Ok(())
}

pub async fn read_info(stream: &mut RecvStream) -> anyhow::Result<Info> {
    let display_width = stream.read_u64().await? as usize;
    let display_height = stream.read_u64().await? as usize;
    let chunks_per_x = stream.read_u64().await? as usize;
    let chunks_per_y = stream.read_u64().await? as usize;
    let thread_count = stream.read_u64().await? as usize;
    let fps = stream.read_f64().await?;

    Ok(Info {
        display_width,
        display_height,
        chunks_per_x,
        chunks_per_y,
        thread_count,
        fps,
    })
}

pub struct Chunk {
    pub x: usize,
    pub y: usize,
    pub hash: u64,
    pub encoded: Box<[u8]>,
    pub encoded_len: usize,
    pub updated: bool,
}

pub async fn write_frame(
    stream: &mut SendStream,
    chunks: &mut [Chunk],
    updated: usize,
) -> anyhow::Result<()> {
    stream.write_u64(updated as u64).await?;

    for chunk in chunks.iter_mut().filter(|c| c.updated) {
        stream.write_u8(chunk.x as u8).await?;
        stream.write_u8(chunk.y as u8).await?;
        stream.write_u64(chunk.encoded_len as u64).await?;
        stream
            .write_all(&chunk.encoded[..chunk.encoded_len])
            .await?;

        chunk.updated = false;
    }

    Ok(())
}

pub fn encode_chunk(
    info: &Info,
    file_offset: usize,
    framebuffer: &[u8],
    encode: &mut [u8],
    chunk: &mut Chunk,
) {
    let frame_top_left_x = chunk.x * info.chunk_width();
    let frame_top_left_y = chunk.y * info.chunk_height();

    let mut hasher = FxHasher::default();

    for row in 0..info.chunk_height() {
        let frame_start =
            (frame_top_left_x + (frame_top_left_y + row) * info.display_width) - file_offset;

        hasher.write(&framebuffer[frame_start..frame_start + info.chunk_width()]);
    }

    let hash = hasher.finish();

    if chunk.hash == hash {
        chunk.updated = false;
        return;
    }

    for row in 0..info.chunk_height() {
        let frame_start =
            (frame_top_left_x + (frame_top_left_y + row) * info.display_width) - file_offset;
        let buffer_start = row * info.chunk_width();

        encode[buffer_start..buffer_start + info.chunk_width()]
            .copy_from_slice(&framebuffer[frame_start..frame_start + info.chunk_width()]);
    }

    chunk.hash = hash;
    chunk.encoded_len = lz4_flex::block::compress_into(encode, &mut chunk.encoded)
        .expect("compression shouldn't fail");

    chunk.updated = true;
}

pub async fn read_frame(
    info: &Info,
    stream: &mut RecvStream,
    encoded: &mut [u8],
    decoded: &mut [u8],
    framebuffer: &Arc<Mutex<Box<[u8]>>>,
) -> anyhow::Result<()> {
    let chunks = stream.read_u64().await?;

    for _ in 0..chunks {
        let x = stream.read_u8().await?;
        let y = stream.read_u8().await?;
        let encoded_len = stream.read_u64().await? as usize;
        stream.read_exact(&mut encoded[..encoded_len]).await?;

        decode_chunk(
            info,
            x,
            y,
            &encoded[..encoded_len],
            decoded,
            &mut framebuffer.lock().unwrap(),
        );
    }

    Ok(())
}

pub fn decode_chunk(
    info: &Info,
    x: u8,
    y: u8,
    encoded: &[u8],
    decoded: &mut [u8],
    framebuffer: &mut [u8],
) {
    lz4_flex::block::decompress_into(encoded, decoded).expect("decompression shouldn't fail");

    let frame_top_left_x = x as usize * info.chunk_width();
    let frame_top_left_y = y as usize * info.chunk_height();

    for row in 0..info.chunk_height() {
        let frame_start = frame_top_left_x + (frame_top_left_y + row) * info.display_width;
        let buffer_start = row * info.chunk_width();

        framebuffer[frame_start..frame_start + info.chunk_width()]
            .copy_from_slice(&decoded[buffer_start..buffer_start + info.chunk_width()]);
    }
}
