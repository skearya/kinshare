use std::{io, os::fd::AsRawFd, thread, time::Duration};

use iroh::{
    Endpoint, SecretKey,
    endpoint::{Connection, QuicTransportConfig, SendStream, presets},
};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use kinshare_shared::{
    consts::ALPN,
    messages::{self, Chunk, Info},
};
use tokio::{
    fs,
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
                .max_idle_timeout(Some(Duration::from_secs(10).try_into()?))
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

impl Stream {
    async fn new(connection: &Connection) -> anyhow::Result<Self> {
        let file = Framebuffer::open().expect("framebuffer failed to open?");

        let info = match fs::read("/mnt/us/extensions/kinshare/stream.json").await {
            Ok(data) => serde_json::from_slice::<Info>(&data)?,
            Err(err) if err.kind() == io::ErrorKind::NotFound => Info {
                display_width: 1872,
                display_height: 2480,
                chunks_per_x: 8,
                chunks_per_y: 8,
                thread_count: 2,
                fps: 60.0,
            },
            Err(err) => return Err(err.into()),
        };

        println!("Starting stream with config: {info:#?}");

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

        messages::write_info(&mut stream, &info).await?;

        Ok(Self {
            info,
            file,
            stream,
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
                            messages::encode_chunk(info, file_offset, framebuffer, buffer, chunk);
                        }
                    });
                }
            });

            let updated = self.chunks.iter().filter(|c| c.updated).count();

            if updated != 0 {
                messages::write_frame(&mut self.stream, &mut self.chunks, updated).await?;
            }
        }
    }
}
