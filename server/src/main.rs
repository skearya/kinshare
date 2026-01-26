use std::{fs, net::UdpSocket, process::Command};

use shared::{codec, messages::Header};

fn main() -> anyhow::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:9921")?;
    println!("Listening on {:#?}", socket.local_addr()?);

    let mut set = 0;
    let mut size = None;
    let mut buffer = vec![0; 1872 * 2480];
    let mut msg = [0; 65535];
    let mut decoded = Vec::with_capacity(1872 * 2480);

    loop {
        let n = socket.recv(&mut msg)?;
        let msg = &msg[..n];

        if n == size_of_val(b"SIZE") + size_of::<u32>()
            && let (b"SIZE", rest) = msg.split_at(size_of_val(b"SIZE"))
        {
            size = Some(u32::from_be_bytes(rest.try_into()?) as usize);
            dbg!(size);
            continue;
        }

        if n <= size_of::<Header>() {
            continue;
        }

        let (start, rest) = msg.split_at(size_of::<u32>());
        let (length, data) = rest.split_at(size_of::<u16>());

        let start = u32::from_be_bytes(start.try_into()?) as usize;
        let length = u16::from_be_bytes(length.try_into()?) as usize;

        if start + length > buffer.len() {
            continue;
        }

        if length != data.len() {
            continue;
        }

        buffer[start..start + length].copy_from_slice(data);

        // TODO: Maintain bitset of set values to avoid counting duplicate writes?
        set += length;

        if let Some(size) = size
            && set == size
        {
            codec::decode(&buffer[..size], &mut decoded);

            fs::write("raw/frame.raw", &decoded)?;
            decoded.clear();

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

            buffer.clear();
            buffer.resize(buffer.capacity(), 0);
        }
    }
}
