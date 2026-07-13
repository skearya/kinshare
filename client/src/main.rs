use std::{hash::Hasher, io, os::fd::AsRawFd, thread, time::Duration};

use iroh::{Endpoint, SecretKey, endpoint::SendStream};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use rustc_hash::FxHasher;
use shared::consts::ALPN;
use tokio::{
    fs,
    io::AsyncWriteExt,
    time::{self, MissedTickBehavior},
};

use crate::framebuffer::Framebuffer;

mod ffi;
mod framebuffer;

const DISPLAY_WIDTH: usize = 1872;
const DISPLAY_HEIGHT: usize = 2480;

struct Client {}

struct Chunk {
    x: usize,
    y: usize,
    hash: u64,
    encoded_len: usize,
    encoded: Box<[u8]>,
    updated: bool,
}

impl Client {
    async fn run() -> anyhow::Result<Self> {
        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .address_lookup(MdnsAddressLookup::builder())
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await?;

        let secret_key = if let Ok(bytes) = fs::read("server.key").await {
            SecretKey::from_bytes(&bytes.as_slice().try_into()?)
        } else {
            panic!("missing server.key")
        };

        dbg!(secret_key.public().to_z32());

        let conn = endpoint.connect(secret_key.public(), ALPN).await?;
        let mut send = conn.open_uni().await?;

        let fb = Framebuffer::open()?;
        let fb_fd = fb.file.as_raw_fd();

        let chunks_per_dimension = 8;

        assert_eq!(DISPLAY_WIDTH % chunks_per_dimension, 0);
        assert_eq!(DISPLAY_HEIGHT % chunks_per_dimension, 0);

        let chunk_width = DISPLAY_WIDTH / chunks_per_dimension;
        let chunk_height = DISPLAY_HEIGHT / chunks_per_dimension;
        let chunk_count = chunks_per_dimension * chunks_per_dimension;

        let mut framebuffer = vec![0u8; DISPLAY_WIDTH * DISPLAY_HEIGHT].into_boxed_slice();
        let mut chunks = (0..chunk_count)
            .map(|i| Chunk {
                x: i % chunks_per_dimension,
                y: i / chunks_per_dimension,
                hash: 0,
                encoded_len: 0,
                encoded: vec![
                    0;
                    lz4_flex::block::get_maximum_output_size(chunk_width * chunk_height)
                ]
                .into_boxed_slice(),
                updated: false,
            })
            .collect::<Box<[Chunk]>>();

        let thread_count = thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(2);
        let thread_bytes = DISPLAY_WIDTH * DISPLAY_HEIGHT / thread_count;

        let mut thread_encode_buffers = vec![vec![0u8; chunk_width * chunk_height]; thread_count];

        let mut interval = time::interval(Duration::from_secs_f64(1.0 / 60.0));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            interval.tick().await;

            thread::scope(|s| {
                for (i, ((framebuffer, chunks), buffer)) in framebuffer
                    .chunks_exact_mut(thread_bytes)
                    .zip(chunks.chunks_exact_mut(chunk_count / thread_count))
                    .zip(thread_encode_buffers.iter_mut())
                    .enumerate()
                {
                    s.spawn(move || {
                        let file_offset = thread_bytes * i;

                        if unsafe {
                            libc::pread(
                                fb_fd,
                                framebuffer.as_mut_ptr().cast(),
                                thread_bytes,
                                file_offset as i64,
                            )
                        } == -1
                        {
                            panic!("pread error: {:?}", io::Error::last_os_error());
                        }

                        for chunk in chunks.iter_mut() {
                            encode_chunk(
                                framebuffer,
                                file_offset,
                                buffer.as_mut_slice(),
                                chunk,
                                chunk_width,
                                chunk_height,
                                DISPLAY_WIDTH,
                            );
                        }
                    });
                }
            });

            let updated = chunks.iter().filter(|c| c.updated).count();

            if updated != 0 {
                write_frame(updated, &mut chunks, &mut send).await?;
            }
        }

        send.finish()?;
        conn.close(0u32.into(), b"bye!");
        endpoint.close().await;

        Ok(Self {})
    }
}

async fn write_frame(
    updated: usize,
    chunks: &mut [Chunk],
    send: &mut SendStream,
) -> anyhow::Result<()> {
    send.write_u64(updated as u64).await?;

    for chunk in chunks.iter_mut().filter(|c| c.updated) {
        send.write_u8(chunk.x as u8).await?;
        send.write_u8(chunk.y as u8).await?;
        send.write_u64(chunk.encoded_len as u64).await?;
        send.write_all(&chunk.encoded[..chunk.encoded_len]).await?;

        chunk.updated = false;
    }

    Ok(())
}

fn encode_chunk(
    framebuffer: &[u8],
    offset: usize,
    buffer: &mut [u8],
    chunk: &mut Chunk,
    chunk_width: usize,
    chunk_height: usize,
    display_width: usize,
) {
    let frame_top_left_x = chunk.x as usize * chunk_width;
    let frame_top_left_y = chunk.y as usize * chunk_height;

    let mut hasher = FxHasher::default();

    for row in 0..chunk_height {
        let frame_start = (frame_top_left_x + (frame_top_left_y + row) * display_width) - offset;

        hasher.write(&framebuffer[frame_start..frame_start + chunk_width]);

        let buffer_start = row * chunk_width;

        buffer[buffer_start..buffer_start + chunk_width]
            .copy_from_slice(&framebuffer[frame_start..frame_start + chunk_width]);
    }

    let hash = hasher.finish();

    if chunk.hash == hash {
        chunk.updated = false;
    } else {
        chunk.hash = hash;
        chunk.encoded_len = lz4_flex::block::compress_into(buffer, &mut chunk.encoded)
            .expect("compression shouldn't fail");

        chunk.updated = true;
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    if cfg!(not(target_os = "linux")) {
        panic!("not running on a kindle?")
    }

    Client::run().await?;

    Ok(())
}
