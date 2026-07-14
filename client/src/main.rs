use std::{hash::Hasher, io, os::fd::AsRawFd, thread, time::Duration};

use iroh::{
    Endpoint, SecretKey,
    endpoint::{Connection, SendStream},
};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use rustc_hash::FxHasher;
use shared::consts::ALPN;
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
    assert!(cfg!(target_os = "linux"), "not running on a kindle?");

    Client::run().await?;

    Ok(())
}

struct Client;

impl Client {
    async fn run() -> anyhow::Result<Self> {
        let secret_key = if let Ok(bytes) = fs::read("kindle.key").await {
            SecretKey::from_bytes(&bytes.as_slice().try_into()?)
        } else {
            panic!("missing kindle.key")
        };

        let endpoint = Endpoint::builder(iroh::endpoint::presets::N0)
            .secret_key(secret_key)
            .address_lookup(MdnsAddressLookup::builder())
            .alpns(vec![ALPN.to_vec()])
            .bind()
            .await?;

        tokio::spawn({
            let endpoint = endpoint.clone();

            async move {
                tokio::signal::ctrl_c().await?;
                endpoint.close().await;

                anyhow::Ok(())
            }
        });

        while let Some(incoming) = endpoint.accept().await {
            let connection = incoming.await?;
            println!("Connected to {}", connection.remote_id());

            let mut stream = match Stream::new(&connection).await {
                Ok(stream) => stream,
                Err(err) => {
                    eprintln!("Error initializing stream: {err:#?}");
                    continue;
                }
            };

            if let Err(err) = stream.run().await {
                eprintln!("Error running stream: {err:#?}");
            }

            connection.close(0u8.into(), &[]);
        }

        Ok(Self {})
    }
}

struct Stream {
    send: SendStream,
    info: Info,
    file: Framebuffer,
    screen: Box<[u8]>,
    chunks: Box<[Chunk]>,
    encode_buffers: Box<[Box<[u8]>]>,
    interval: Interval,
}

struct Info {
    display_width: usize,
    display_height: usize,
    chunks_per_x: usize,
    chunks_per_y: usize,
    thread_count: usize,
    fps: f64,
}

struct Chunk {
    x: usize,
    y: usize,
    hash: u64,
    encoded: Box<[u8]>,
    encoded_len: usize,
    updated: bool,
}

impl Info {
    fn display_size(&self) -> usize {
        self.display_width * self.display_height
    }

    fn chunk_width(&self) -> usize {
        self.display_width / self.chunks_per_x
    }

    fn chunk_height(&self) -> usize {
        self.display_height / self.chunks_per_y
    }

    fn chunk_count(&self) -> usize {
        self.chunks_per_x * self.chunks_per_y
    }

    fn chunk_size(&self) -> usize {
        self.chunk_width() * self.chunk_height()
    }
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

        let send = connection.open_uni().await?;

        Ok(Self {
            send,
            info,
            file,
            screen,
            chunks,
            encode_buffers,
            interval,
        })
    }

    async fn run(&mut self) -> anyhow::Result<()> {
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
                write_frame(updated, &mut self.chunks, &mut self.send).await?;
            }
        }
    }
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
