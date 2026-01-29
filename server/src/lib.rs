use std::{
    collections::HashSet,
    mem,
    net::UdpSocket,
    ops,
    sync::{
        Arc, Mutex,
        atomic::{self, AtomicBool},
    },
};

use shared::{codec, messages::Header};

pub struct Server {
    frame: usize,
    size: usize,

    set: usize,
    placed: HashSet<ops::Range<usize>>,
    staging: Vec<u8>,

    front: Arc<Mutex<Vec<u8>>>,
    back: Vec<u8>,
    changed: Arc<AtomicBool>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            frame: 0,
            size: 0,
            set: 0,
            placed: HashSet::new(),
            staging: Vec::with_capacity(1872 * 2480),
            front: Arc::new(Mutex::new(Vec::with_capacity(1872 * 2480))),
            back: Vec::with_capacity(1872 * 2480),
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
            let (size, rest) = rest.split_at(size_of::<u32>());
            let (offset, rest) = rest.split_at(size_of::<u32>());
            let (length, data) = rest.split_at(size_of::<u16>());

            let frame = u32::from_be_bytes(frame.try_into()?) as usize;
            let size = u32::from_be_bytes(size.try_into()?) as usize;
            let offset = u32::from_be_bytes(offset.try_into()?) as usize;
            let length = u16::from_be_bytes(length.try_into()?) as usize;

            if length != data.len() {
                continue;
            }

            if offset + length > size {
                continue;
            }

            self.message(frame, size, offset, length, data)?;
        }
    }

    fn message(
        &mut self,
        frame: usize,
        size: usize,
        offset: usize,
        length: usize,
        data: &[u8],
    ) -> anyhow::Result<()> {
        if frame < self.frame {
            return Ok(());
        } else if frame > self.frame {
            println!("{}, {}, {}, {}", frame, self.frame, self.set, self.size);
            self.clear();

            self.frame = frame;
            self.size = size;
            self.staging.resize(size, 0);
        }

        if !self.placed.insert(offset..offset + length) {
            return Ok(());
        }

        self.set += length;
        self.staging[offset..offset + length].copy_from_slice(data);

        if self.set == self.size {
            codec::decode(&self.staging[..self.size], &mut self.back);

            {
                let mut front = self.front.lock().unwrap();
                mem::swap(&mut self.back, &mut front);
            }

            self.changed.store(true, atomic::Ordering::SeqCst);

            self.clear();
        }

        Ok(())
    }

    fn clear(&mut self) {
        self.set = 0;
        self.staging.clear();
        self.placed.clear();
    }

    pub fn front(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.front)
    }

    pub fn changed(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.changed)
    }
}
