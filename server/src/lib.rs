use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use iroh::{
    Endpoint, SecretKey,
    endpoint::{Connection, QuicTransportConfig, RecvStream, presets},
};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use kinshare_shared::consts::ALPN;
use tokio::{fs, io::AsyncReadExt, sync::mpsc};

const CHUNK_WIDTH: usize = 1872 / 8;
const CHUNK_HEIGHT: usize = 2480 / 8;
const DISPLAY_WIDTH: usize = 1872;
const DISPLAY_HEIGHT: usize = 2480;

#[derive(Debug, Clone)]
pub enum Message {
    Info(&'static str),
    Connected(Arc<Mutex<Box<[u8]>>>),
    Updated,
    Closed,
}

pub async fn run(sender: mpsc::UnboundedSender<Message>) -> anyhow::Result<()> {
    let (server_key, kindle_key) = if let Ok(bytes) = fs::read("connection.keys").await {
        (
            SecretKey::from_bytes(&bytes[..32].try_into()?),
            SecretKey::from_bytes(&bytes[32..64].try_into()?),
        )
    } else {
        let server_key = SecretKey::generate();
        let kindle_key = SecretKey::generate();

        let data = [server_key.to_bytes(), kindle_key.to_bytes()].concat();

        fs::write("connection.keys", data).await?;

        sender.send(
            Message::Info("Wrote connection information to 'connection.keys'. Share it with the kindle in '/mnt/us/extensions/kinshare/connection.keys'.")
        )?;

        (server_key, kindle_key)
    };

    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(server_key)
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

    loop {
        sender.send(Message::Info("Connecting..."))?;

        let Ok(connection) = endpoint.connect(kindle_key.public(), ALPN).await else {
            continue;
        };

        println!("Connected to {}", connection.remote_id());

        match Stream::new(&sender, &connection).await {
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
}

struct Stream<'a> {
    sender: &'a mpsc::UnboundedSender<Message>,
    stream: RecvStream,
    framebuffer: Arc<Mutex<Box<[u8]>>>,
    encode_buffer: Box<[u8]>,
    decode_buffer: Box<[u8]>,
}

impl<'a> Stream<'a> {
    async fn new(
        sender: &'a mpsc::UnboundedSender<Message>,
        connection: &Connection,
    ) -> anyhow::Result<Self> {
        let stream = connection.accept_uni().await?;

        let framebuffer = Arc::new(Mutex::new(
            vec![0; DISPLAY_WIDTH * DISPLAY_HEIGHT].into_boxed_slice(),
        ));

        let encode_buffer =
            vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_WIDTH * CHUNK_HEIGHT)]
                .into_boxed_slice();

        let decode_buffer = vec![0; CHUNK_WIDTH * CHUNK_HEIGHT].into_boxed_slice();

        sender.send(Message::Connected(Arc::clone(&framebuffer)))?;

        Ok(Self {
            sender,
            stream,
            framebuffer,
            encode_buffer,
            decode_buffer,
        })
    }

    async fn run(mut self) -> anyhow::Result<()> {
        loop {
            read_frame(
                &mut self.stream,
                &mut self.encode_buffer,
                &mut self.decode_buffer,
                &self.framebuffer,
            )
            .await?;

            self.sender.send(Message::Updated)?;
        }
    }
}

async fn read_frame(
    stream: &mut RecvStream,
    encode_buffer: &mut [u8],
    decode_buffer: &mut [u8],
    framebuffer: &Arc<Mutex<Box<[u8]>>>,
) -> anyhow::Result<()> {
    let chunks = stream.read_u64().await?;

    for _ in 0..chunks {
        let x = stream.read_u8().await?;
        let y = stream.read_u8().await?;
        let encoded_len = stream.read_u64().await? as usize;
        stream.read_exact(&mut encode_buffer[..encoded_len]).await?;

        let mut lock = framebuffer.lock().unwrap();

        decode_chunk(
            &mut lock,
            decode_buffer,
            x,
            y,
            &encode_buffer[..encoded_len],
        );
    }

    Ok(())
}

pub fn decode_chunk(framebuffer: &mut [u8], decoded: &mut [u8], x: u8, y: u8, data: &[u8]) {
    lz4_flex::block::decompress_into(data, decoded).expect("decompression shouldn't fail");

    let frame_top_left_x = x as usize * CHUNK_WIDTH;
    let frame_top_left_y = y as usize * CHUNK_HEIGHT;

    for row in 0..CHUNK_HEIGHT {
        let frame_start = frame_top_left_x + (frame_top_left_y + row) * DISPLAY_WIDTH;
        let buffer_start = row * CHUNK_WIDTH;

        framebuffer[frame_start..frame_start + CHUNK_WIDTH]
            .copy_from_slice(&decoded[buffer_start..buffer_start + CHUNK_WIDTH]);
    }
}
