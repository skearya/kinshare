use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use iroh::{
    Endpoint, SecretKey,
    endpoint::{Connection, QuicTransportConfig, RecvStream, presets},
};
use iroh_mdns_address_lookup::MdnsAddressLookup;
use kinshare_shared::{consts::ALPN, messages};
use tokio::{fs, sync::mpsc};

#[derive(Debug, Clone)]
pub enum Message {
    Message(&'static str),
    Connected {
        info: messages::Info,
        framebuffer: Arc<Mutex<Box<[u8]>>>,
    },
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
            Message::Message("Wrote connection information to 'connection.keys'. Share it with the kindle in '/mnt/us/extensions/kinshare/connection.keys'.")
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

    sender.send(Message::Message("Connecting..."))?;

    loop {
        let Ok(connection) = endpoint.connect(kindle_key.public(), ALPN).await else {
            sender.send(Message::Message("Retrying..."))?;
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
    info: messages::Info,
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
        let mut stream = connection.accept_uni().await?;

        let info = messages::read_info(&mut stream).await?;

        let framebuffer = Arc::new(Mutex::new(vec![0; info.display_size()].into_boxed_slice()));

        let encode_buffer =
            vec![0; lz4_flex::block::get_maximum_output_size(info.chunk_size())].into_boxed_slice();

        let decode_buffer = vec![0; info.chunk_size()].into_boxed_slice();

        sender.send(Message::Connected {
            info: info.clone(),
            framebuffer: Arc::clone(&framebuffer),
        })?;

        Ok(Self {
            stream,
            info,
            sender,
            framebuffer,
            encode_buffer,
            decode_buffer,
        })
    }

    async fn run(mut self) -> anyhow::Result<()> {
        loop {
            messages::read_frame(
                &self.info,
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
