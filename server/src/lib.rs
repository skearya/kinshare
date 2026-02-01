use std::{
    array, mem,
    net::UdpSocket,
    sync::{
        Arc, Mutex,
        atomic::{self, AtomicBool},
    },
};

use shared::{
    codec,
    consts::{
        CHUNK_HEIGHT, CHUNK_SIZE, CHUNK_WIDTH, DISPLAY_HEIGHT, DISPLAY_SIZE, DISPLAY_WIDTH,
        NUM_CHUNKS,
    },
    messages::Header,
};

#[derive(Clone)]
struct Chunk {
    frame: u32,
    recieved: u32,
    encoded: Vec<u8>,
}

pub struct Server {
    chunks: [Chunk; NUM_CHUNKS],
    decoded: Vec<u8>,

    back: Vec<u8>,
    front: Arc<Mutex<Vec<u8>>>,
    changed: Arc<AtomicBool>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            chunks: array::repeat(Chunk {
                frame: 0,
                recieved: 0,
                encoded: vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)],
            }),
            decoded: vec![0; CHUNK_SIZE],
            back: vec![0; DISPLAY_SIZE],
            front: Arc::new(Mutex::new(vec![0; DISPLAY_SIZE])),
            changed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:9921")?;
        println!("Listening on {:#?}", socket.local_addr()?);

        let mut msg = [0; 65535];

        loop {
            let n = socket.recv(&mut msg)?;
            let msg = &msg[..n];

            if n <= size_of::<Header>() {
                continue;
            }

            let (frame, rest) = msg.split_at(size_of::<u32>());
            let (offset, rest) = rest.split_at(size_of::<u32>());
            let (total, rest) = rest.split_at(size_of::<u32>());
            let (x, rest) = rest.split_at(size_of::<u8>());
            let (y, rest) = rest.split_at(size_of::<u8>());

            let frame = u32::from_be_bytes(frame.try_into()?);
            let offset = u32::from_be_bytes(offset.try_into()?);
            let total = u32::from_be_bytes(total.try_into()?);
            let x = u8::from_be_bytes(x.try_into()?);
            let y = u8::from_be_bytes(y.try_into()?);

            self.message(frame, offset, total, x, y, rest)?;
        }
    }

    fn message(
        &mut self,
        frame: u32,
        offset: u32,
        total: u32,
        x: u8,
        y: u8,
        data: &[u8],
    ) -> anyhow::Result<()> {
        if !(0..(DISPLAY_WIDTH / CHUNK_WIDTH) as u8).contains(&x) {
            return Ok(());
        }

        if !(0..(DISPLAY_HEIGHT / CHUNK_HEIGHT) as u8).contains(&y) {
            return Ok(());
        }

        let chunk = &mut self.chunks[y as usize * (DISPLAY_WIDTH / CHUNK_WIDTH) + x as usize];

        if frame < chunk.frame {
            return Ok(());
        } else if frame > chunk.frame {
            chunk.frame = frame;
            chunk.recieved = 0;
        }

        chunk.encoded[offset as usize..offset as usize + data.len()].copy_from_slice(data);
        chunk.recieved += data.len() as u32;

        if chunk.recieved == total {
            {
                let mut front = self.front.lock().unwrap();

                codec::decode(
                    &mut front,
                    &mut self.decoded,
                    x,
                    y,
                    &chunk.encoded[..total as usize],
                );
            }

            self.changed.store(true, atomic::Ordering::SeqCst);
        } else {
            println!("{} / {}", chunk.recieved, total);
        }

        Ok(())
    }

    pub fn front(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.front)
    }

    pub fn changed(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.changed)
    }
}
