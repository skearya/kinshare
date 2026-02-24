#![feature(thread_sleep_until)]

mod framebuffer;

use std::fs::File;
use std::net::UdpSocket;
use std::os::unix::prelude::*;
use std::time::{Duration, Instant};
use std::{array, io, ptr, thread};

use shared::codec;
use shared::codec::Chunk;
use shared::consts::{
    CHUNK_HEIGHT, CHUNK_SIZE, CHUNK_WIDTH, DISPLAY_HEIGHT, DISPLAY_SIZE, DISPLAY_WIDTH, NUM_CHUNKS,
    PACKET_SIZE,
};
use shared::messages::Header;

const ADDR: &str = "192.168.15.245:9921";
const MAX_FPS: f32 = 60.0;

struct Client {
    fb0: File,
    socket: UdpSocket,

    framebuffer: Vec<u8>,
    chunks: [Chunk; NUM_CHUNKS],

    headers: Vec<Header>,
    iovecs: Vec<libc::iovec>,
    msghdrs: Vec<libc::mmsghdr>,

    frame: u32,
    frame_time: Duration,
    next_frame: Instant,
}

impl Client {
    fn new() -> anyhow::Result<Self> {
        let fb0 = File::open("/dev/fb0")?;
        let framebuffer = vec![0; DISPLAY_SIZE];

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect(ADDR)?;

        let chunks = array::from_fn(|i| Chunk {
            x: (i % (DISPLAY_WIDTH / CHUNK_WIDTH)) as u8,
            y: (i / (DISPLAY_HEIGHT / CHUNK_HEIGHT)) as u8,
            encoded: vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)],
            hash: 0,
            size: 0,
        });

        // Buffers for sendmmsg().
        // If any of these resize unexpectedly, we will segfault!
        let headers: Vec<Header> = vec![];
        let iovecs: Vec<libc::iovec> = vec![];
        let msghdrs: Vec<libc::mmsghdr> = vec![];

        let frame: u32 = 0;
        let frame_time = Duration::from_secs_f32(1.0 / MAX_FPS);
        let next_frame = Instant::now();

        Ok(Self {
            fb0,
            socket,
            framebuffer,
            chunks,
            headers,
            iovecs,
            msghdrs,
            frame,
            frame_time,
            next_frame,
        })
    }

    pub fn run(mut self) -> anyhow::Result<()> {
        loop {
            thread::sleep_until(self.next_frame);
            self.next_frame += self.frame_time;

            self.frame()?;
        }
    }

    fn frame(&mut self) -> anyhow::Result<()> {
        let changed = codec::encode(
            self.fb0.as_raw_fd(),
            &mut self.framebuffer,
            &mut self.chunks,
        );

        let count = changed
            .iter()
            .map(|&change| if change { 1 } else { 0 })
            .sum::<usize>();

        if count != 0 {
            let messages = self
                .chunks
                .iter()
                .zip(changed)
                .filter_map(|(chunk, changed)| if changed { Some(chunk) } else { None })
                .map(|chunk| chunk.size.div_ceil(PACKET_SIZE - size_of::<Header>()))
                .sum::<usize>();

            self.headers
                .reserve_exact(messages.saturating_sub(self.headers.len()));
            self.iovecs
                .reserve_exact((messages * 2).saturating_sub(self.iovecs.len()));
            self.msghdrs
                .reserve_exact(messages.saturating_sub(self.msghdrs.len()));

            for chunk in self
                .chunks
                .iter_mut()
                .zip(changed)
                .filter_map(|(chunk, changed)| if changed { Some(chunk) } else { None })
            {
                let mut offset: u32 = 0;

                for part in
                    chunk.encoded[..chunk.size].chunks_mut(PACKET_SIZE - size_of::<Header>())
                {
                    let header = self.headers.push_mut(Header {
                        frame: self.frame.to_be_bytes(),
                        chunks: (count as u32).to_be_bytes(),
                        x: chunk.x.to_be_bytes(),
                        y: chunk.y.to_be_bytes(),
                        size: (chunk.size as u32).to_be_bytes(),
                        offset: offset.to_be_bytes(),
                    });

                    let len = self.iovecs.len();

                    self.iovecs.push(libc::iovec {
                        iov_base: (header as *mut Header).cast(),
                        iov_len: size_of::<Header>(),
                    });

                    self.iovecs.push(libc::iovec {
                        iov_base: part.as_mut_ptr().cast(),
                        iov_len: part.len(),
                    });

                    self.msghdrs.push(libc::mmsghdr {
                        msg_hdr: libc::msghdr {
                            msg_name: ptr::null_mut(),
                            msg_namelen: 0,
                            msg_iov: self.iovecs[len..].as_mut_ptr(),
                            msg_iovlen: 2,
                            msg_control: ptr::null_mut(),
                            msg_controllen: 0,
                            msg_flags: 0,
                        },
                        msg_len: 0,
                    });

                    offset += part.len() as u32;
                }
            }

            let mut sent = 0;

            while sent != self.msghdrs.len() {
                let n = unsafe {
                    libc::sendmmsg(
                        self.socket.as_raw_fd(),
                        self.msghdrs[sent..].as_mut_ptr(),
                        self.msghdrs[sent..].len().try_into()?,
                        libc::MSG_NOSIGNAL.try_into()?,
                    )
                };

                if n == -1 {
                    panic!("sendmmsg error: {:?}", io::Error::last_os_error());
                }

                sent += n as usize;
            }

            self.headers.clear();
            self.iovecs.clear();
            self.msghdrs.clear();

            self.frame += 1;
        }

        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    Client::new()?.run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    panic!("not running on a kindle?")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec() -> anyhow::Result<()> {
        let fb0 = File::open("raw/frame.raw")?;

        let mut framebuffer = vec![0; DISPLAY_SIZE];
        let mut chunks: [Chunk; NUM_CHUNKS] = array::from_fn(|i| Chunk {
            x: (i % (DISPLAY_WIDTH / CHUNK_WIDTH)) as u8,
            y: (i / (DISPLAY_HEIGHT / CHUNK_HEIGHT)) as u8,
            encoded: vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)],
            hash: 0,
            size: 0,
        });

        let changed = codec::encode(fb0.as_raw_fd(), &mut framebuffer, &mut chunks);
        assert_eq!(changed, [true; 64]);

        let mut decoded = vec![0; DISPLAY_SIZE];
        let mut decode_buffer = vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)];

        for chunk in &mut chunks {
            codec::decode(
                &mut decoded,
                &mut decode_buffer,
                chunk.x,
                chunk.y,
                &chunk.encoded[..chunk.size],
            );
        }

        assert!(framebuffer == decoded);

        Ok(())
    }
}
