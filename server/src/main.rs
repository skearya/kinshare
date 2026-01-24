use std::{fs, net::UdpSocket, process::Command};

fn main() -> anyhow::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:9921")?;
    println!("Listening on {:#?}", socket.local_addr()?);

    let mut buffer = vec![];
    let mut msg = [0; 65535];

    loop {
        let n = socket.recv(&mut msg)?;

        buffer.extend_from_slice(&msg[..n]);

        if buffer.len() >= 1872 * 2480 {
            fs::write("raw/frame.raw", &buffer)?;
            buffer.clear();

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

            println!("written frame!");
        }
    }
}
