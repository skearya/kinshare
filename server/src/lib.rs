use std::sync::{Arc, Mutex};

use iroh::{
    Endpoint, SecretKey,
    endpoint::{RecvStream, presets},
};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use tokio::{fs, io::AsyncReadExt, sync::mpsc};

use shared::consts::ALPN;

const CHUNK_WIDTH: usize = 1872 / 8;
const CHUNK_HEIGHT: usize = 2480 / 8;
const DISPLAY_WIDTH: usize = 1872;
const DISPLAY_HEIGHT: usize = 2480;

#[derive(Debug, Clone)]
pub enum Message {
    Screen(Arc<Mutex<Vec<u8>>>),
    Updated,
}

pub async fn run(sender: mpsc::UnboundedSender<Message>) -> anyhow::Result<()> {
    let secret_key = if let Ok(bytes) = fs::read("kindle.key").await {
        SecretKey::from_bytes(&bytes.as_slice().try_into()?)
    } else {
        // Treat this file like a password: anyone with it can
        // impersonate your endpoint. Store it securely.
        fs::write("kindle.key", SecretKey::generate().to_bytes()).await?;

        panic!(
            "Wrote kindle information to 'kindle.key', share it with the kindle before running the client."
        );
    };

    let endpoint = Endpoint::builder(presets::N0)
        .address_lookup(MdnsAddressLookup::builder())
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await?;

    println!("Our endpoint id: {}", endpoint.id().to_z32());

    let framebuffer = Arc::new(Mutex::new(vec![0u8; DISPLAY_WIDTH * DISPLAY_HEIGHT]));
    sender.send(Message::Screen(Arc::clone(&framebuffer)))?;

    let connection = endpoint.connect(secret_key.public(), ALPN).await?;
    println!("Connected to {}", connection.remote_id());

    let mut stream = connection.accept_uni().await?;

    loop {
        match read_frame(&mut stream, &framebuffer).await {
            Ok(()) => sender.send(Message::Updated)?,
            Err(err) => {
                println!("Error reading frame: {err:#?}");
                break;
            }
        }
    }

    stream.stop(0u8.into())?;
    connection.close(0u8.into(), &[]);

    Ok(())
}

async fn read_frame(
    stream: &mut RecvStream,
    framebuffer: &Arc<Mutex<Vec<u8>>>,
) -> anyhow::Result<()> {
    let chunks = stream.read_u64().await?;

    let mut encoded = vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_WIDTH * CHUNK_HEIGHT)];
    let mut decoded = vec![0; CHUNK_WIDTH * CHUNK_HEIGHT];

    for _ in 0..chunks {
        let x = stream.read_u8().await?;
        let y = stream.read_u8().await?;
        let encoded_len = stream.read_u64().await? as usize;
        stream.read_exact(&mut encoded[..encoded_len]).await?;

        {
            let mut lock = framebuffer.lock().unwrap();

            decode_chunk(&mut lock, &mut decoded, x, y, &encoded[..encoded_len]);
        }
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
