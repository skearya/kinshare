use std::{hash::Hasher, io, os::fd::AsRawFd, thread, time::Duration};

use iroh::{
    Endpoint, SecretKey,
    endpoint::{Connection, QuicTransportConfig, SendStream, presets},
};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use kinshare_shared::{Info, consts::ALPN};
use rustc_hash::FxHasher;
use tokio::{
    fs,
    io::AsyncWriteExt,
    time::{self, Interval, MissedTickBehavior},
};

use crate::framebuffer::Framebuffer;

mod ffi;
mod framebuffer;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    const { assert!(cfg!(target_os = "linux"), "not running on a kindle?") }

    run().await
}

async fn run() -> anyhow::Result<()> {
    let (server_key, kindle_key) = if let Ok(bytes) = fs::read("connection.keys").await {
        (
            SecretKey::from_bytes(&bytes[..32].try_into()?),
            SecretKey::from_bytes(&bytes[32..64].try_into()?),
        )
    } else {
        panic!("missing 'connection.keys' file")
    };

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(kindle_key)
        .address_lookup(MdnsAddressLookup::builder())
        .transport_config(
            QuicTransportConfig::builder()
                .max_idle_timeout(Some(Duration::from_secs(5).try_into()?))
                .build(),
        )
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;

    println!("Endpoint id: {}", endpoint.id().to_z32());

    while let Some(incoming) = endpoint.accept().await {
        let Ok(connection) = incoming.await else {
            continue;
        };

        if server_key.public() != connection.remote_id() {
            println!(
                "Non-server tried connecting to us: {}",
                connection.remote_id()
            );

            connection.close(0u8.into(), b"Unauthorized");
            continue;
        }

        println!("Connected to: {}", connection.remote_id());

        match Stream::new(&connection).await {
            Ok(stream) => {
                if let Err(err) = stream.run().await {
                    eprintln!("Error running stream: {err:#?}");
                }
            }
            Err(err) => {
                eprintln!("Error initializing stream: {err:#?}");
            }
        }

        connection.close(0u8.into(), &[]);
    }

    Ok(())
}

struct Stream {
    info: Info,
    file: Framebuffer,
    stream: SendStream,
    screen: Box<[u8]>,
    chunks: Box<[Chunk]>,
    encode_buffers: Box<[Box<[u8]>]>,
    interval: Interval,
}

struct Chunk {
    x: usize,
    y: usize,
    hash: u64,
    encoded: Box<[u8]>,
    encoded_len: usize,
    updated: bool,
}

impl Stream {
    async fn new(connection: &Connection) -> anyhow::Result<Self> {
        let file = Framebuffer::open().expect("framebuffer failed to open?");

        let info = Info {
            display_width: 1872,
            display_height: 2480,
            chunks_per_x: 8,
            chunks_per_y: 8,
            thread_count: 2,
            fps: 60.0,
        };

        assert_eq!(info.display_width % info.chunks_per_x, 0);
        assert_eq!(info.display_height % info.chunks_per_y, 0);

        let screen = vec![0; info.display_size()].into_boxed_slice();

        let chunks = (0..info.chunk_count())
            .map(|i| Chunk {
                x: i % info.chunks_per_x,
                y: i / info.chunks_per_y,
                hash: 0,
                encoded: vec![0; lz4_flex::block::get_maximum_output_size(info.chunk_size())]
                    .into_boxed_slice(),
                encoded_len: 0,
                updated: false,
            })
            .collect::<Box<[Chunk]>>();

        let encode_buffers = vec![vec![0; info.chunk_size()].into_boxed_slice(); info.thread_count]
            .into_boxed_slice();

        let mut interval = time::interval(Duration::from_secs_f64(1.0 / info.fps));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let mut stream = connection.open_uni().await?;

        write_info(&mut stream, &info).await?;

        Ok(Self {
            stream,
            info,
            file,
            screen,
            chunks,
            encode_buffers,
            interval,
        })
    }

    async fn run(mut self) -> anyhow::Result<()> {
        loop {
            self.interval.tick().await;

            let info = &self.info;
            let thread_chunks = info.chunk_count() / info.thread_count;
            let thread_size = info.display_size() / info.thread_count;

            let fb_fd = self.file.file.as_raw_fd();

            thread::scope(|s| {
                for (i, ((framebuffer, chunks), buffer)) in self
                    .screen
                    .chunks_exact_mut(thread_size)
                    .zip(self.chunks.chunks_exact_mut(thread_chunks))
                    .zip(self.encode_buffers.iter_mut())
                    .enumerate()
                {
                    s.spawn(move || {
                        let file_offset = thread_size * i;

                        if unsafe {
                            libc::pread(
                                fb_fd,
                                framebuffer.as_mut_ptr().cast(),
                                thread_size,
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
                                buffer,
                                chunk,
                                info.chunk_width(),
                                info.chunk_height(),
                                info.display_width,
                            );
                        }
                    });
                }
            });

            let updated = self.chunks.iter().filter(|c| c.updated).count();

            if updated != 0 {
                write_frame(&mut self.stream, &mut self.chunks, updated).await?;
            }
        }
    }
}

async fn write_info(stream: &mut SendStream, info: &Info) -> anyhow::Result<()> {
    stream.write_u64(info.display_width as u64).await?;
    stream.write_u64(info.display_height as u64).await?;
    stream.write_u64(info.chunks_per_x as u64).await?;
    stream.write_u64(info.chunks_per_y as u64).await?;
    stream.write_u64(info.thread_count as u64).await?;
    stream.write_f64(info.fps).await?;

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
    let frame_top_left_x = chunk.x * chunk_width;
    let frame_top_left_y = chunk.y * chunk_height;

    let mut hasher = FxHasher::default();

    for row in 0..chunk_height {
        let frame_start = (frame_top_left_x + (frame_top_left_y + row) * display_width) - offset;

        hasher.write(&framebuffer[frame_start..frame_start + chunk_width]);
    }

    let hash = hasher.finish();

    if chunk.hash == hash {
        chunk.updated = false;
        return;
    }

    for row in 0..chunk_height {
        let frame_start = (frame_top_left_x + (frame_top_left_y + row) * display_width) - offset;
        let buffer_start = row * chunk_width;

        buffer[buffer_start..buffer_start + chunk_width]
            .copy_from_slice(&framebuffer[frame_start..frame_start + chunk_width]);
    }

    chunk.hash = hash;
    chunk.encoded_len = lz4_flex::block::compress_into(buffer, &mut chunk.encoded)
        .expect("compression shouldn't fail");

    chunk.updated = true;
}

async fn write_frame(
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
