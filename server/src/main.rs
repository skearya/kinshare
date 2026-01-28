use std::{collections::HashSet, fs, net::UdpSocket, ops, process::Command};

use shared::{codec, messages::Header};

#[derive(Default)]
struct Server {
    frame: usize,
    size: usize,
    set: usize,
    placed: HashSet<ops::Range<usize>>,
    buffer: Vec<u8>,
    decoded: Vec<u8>,
}

impl Server {
    fn on_message(
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
            self.clear();

            self.frame = frame;
            self.size = size;
            self.buffer.resize(size, 0);
        }

        if !self.placed.insert(offset..offset + length) {
            return Ok(());
        }

        self.set += length;
        self.buffer[offset..offset + length].copy_from_slice(data);

        if self.set == self.size {
            println!("Processing, size = {}", self.set);

            codec::decode(&self.buffer[..self.size], &mut self.decoded);
            fs::write("raw/frame.raw", &self.decoded)?;

            Command::new("magick")
                .args([
                    "-size",
                    "1872x2480",
                    "-depth",
                    "8",
                    "gray:raw/frame.raw",
                    "out/frame.png",
                ])
                .spawn()?;

            self.clear();
        }

        Ok(())
    }

    fn clear(&mut self) {
        self.set = 0;
        self.buffer.clear();
        self.placed.clear();
    }
}

fn main() -> anyhow::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:9921")?;
    println!("Listening on {:#?}", socket.local_addr()?);

    let mut server = Server::default();
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

        server.on_message(frame, size, offset, length, data)?;
    }
}
