use std::{
    array,
    net::UdpSocket,
    sync::{Arc, Mutex, mpsc},
    thread,
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
    recieved: u32,
    encoded: Vec<u8>,
}

pub struct Server {
    frame: u32,

    chunks: [Chunk; NUM_CHUNKS],
    decoded: Vec<u8>,
    changed: Vec<(u8, u8)>,

    front: Arc<Mutex<Vec<u8>>>,
    notifier: mpsc::Sender<Vec<(u8, u8)>>,
}

impl Server {
    pub fn spawn() -> (Arc<Mutex<Vec<u8>>>, mpsc::Receiver<Vec<(u8, u8)>>) {
        let front = Arc::new(Mutex::new(vec![0; DISPLAY_SIZE]));
        let (sender, reciever) = mpsc::channel();

        let server = Self {
            frame: 0,
            chunks: array::repeat(Chunk {
                recieved: 0,
                encoded: vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)],
            }),
            decoded: vec![0; CHUNK_SIZE],
            changed: vec![],
            front: Arc::clone(&front),
            notifier: sender,
        };

        thread::spawn(|| server.run().expect("server crashed"));

        (front, reciever)
    }

    fn run(mut self) -> anyhow::Result<()> {
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
            let (chunks, rest) = rest.split_at(size_of::<u32>());
            let (x, rest) = rest.split_at(size_of::<u8>());
            let (y, rest) = rest.split_at(size_of::<u8>());
            let (size, rest) = rest.split_at(size_of::<u32>());
            let (offset, rest) = rest.split_at(size_of::<u32>());

            let frame = u32::from_be_bytes(frame.try_into()?);
            let chunks = u32::from_be_bytes(chunks.try_into()?);
            let x = u8::from_be_bytes(x.try_into()?);
            let y = u8::from_be_bytes(y.try_into()?);
            let size = u32::from_be_bytes(size.try_into()?);
            let offset = u32::from_be_bytes(offset.try_into()?);

            self.message(frame, chunks, x, y, size, offset, rest)?;
        }
    }

    fn message(
        &mut self,
        frame: u32,
        chunks: u32,
        x: u8,
        y: u8,
        size: u32,
        offset: u32,
        data: &[u8],
    ) -> anyhow::Result<()> {
        if !(0..(DISPLAY_WIDTH / CHUNK_WIDTH) as u8).contains(&x) {
            return Ok(());
        }

        if !(0..(DISPLAY_HEIGHT / CHUNK_HEIGHT) as u8).contains(&y) {
            return Ok(());
        }

        if frame < self.frame {
            return Ok(());
        } else if frame > self.frame {
            self.frame = frame;
            self.changed.clear();

            for chunk in &mut self.chunks {
                chunk.recieved = 0;
            }
        }

        let chunk = &mut self.chunks[y as usize * (DISPLAY_WIDTH / CHUNK_WIDTH) + x as usize];

        chunk.encoded[offset as usize..offset as usize + data.len()].copy_from_slice(data);
        chunk.recieved += data.len() as u32;

        if chunk.recieved == size {
            {
                let mut front = self.front.lock().unwrap();

                codec::decode(
                    &mut front,
                    &mut self.decoded,
                    x,
                    y,
                    &chunk.encoded[..size as usize],
                );
            }

            self.changed.push((x, y));

            if self.changed.len() as u32 == chunks {
                self.notifier.send(self.changed.clone()).unwrap();
            }
        }

        Ok(())
    }
}
