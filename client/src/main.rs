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

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect(ADDR)?;

    let fb0 = File::open("/dev/fb0")?;
    let mut framebuffer = vec![0; DISPLAY_SIZE];
    let mut chunks: [Chunk; NUM_CHUNKS] = array::from_fn(|i| Chunk {
        x: (i % (DISPLAY_WIDTH / CHUNK_WIDTH)) as u8,
        y: (i / (DISPLAY_HEIGHT / CHUNK_HEIGHT)) as u8,
        encoded: vec![0; lz4_flex::block::get_maximum_output_size(CHUNK_SIZE)],
        hash: 0,
        size: 0,
    });

    // Reused buffers for sendmmsg().
    // If any of these resize unexpectedly, we will segfault!
    let mut headers: Vec<Header> = vec![];
    let mut iovecs: Vec<libc::iovec> = vec![];
    let mut msghdrs: Vec<libc::mmsghdr> = vec![];
    let mut frame: u32 = 0;

    let frame_time = Duration::from_secs_f32(1.0 / MAX_FPS);
    let mut next_frame = Instant::now();

    loop {
        let changed = codec::encode(fb0.as_raw_fd(), &mut framebuffer, &mut chunks);
        let count = changed
            .iter()
            .map(|&change| if change { 1 } else { 0 })
            .sum::<usize>();

        if count != 0 {
            let messages = chunks
                .iter()
                .zip(changed)
                .filter_map(|(chunk, changed)| if changed { Some(chunk) } else { None })
                .map(|chunk| chunk.size.div_ceil(PACKET_SIZE - size_of::<Header>()))
                .sum::<usize>();

            headers.reserve_exact(messages.saturating_sub(headers.len()));
            iovecs.reserve_exact((messages * 2).saturating_sub(iovecs.len()));
            msghdrs.reserve_exact(messages.saturating_sub(msghdrs.len()));

            for chunk in chunks
                .iter_mut()
                .zip(changed)
                .filter_map(|(chunk, changed)| if changed { Some(chunk) } else { None })
            {
                let mut offset: u32 = 0;

                for part in
                    chunk.encoded[..chunk.size].chunks_mut(PACKET_SIZE - size_of::<Header>())
                {
                    let header = headers.push_mut(Header {
                        frame: frame.to_be_bytes(),
                        offset: offset.to_be_bytes(),
                        total: (chunk.size as u32).to_be_bytes(),
                        x: chunk.x.to_be_bytes(),
                        y: chunk.y.to_be_bytes(),
                    });

                    let len = iovecs.len();

                    iovecs.push(libc::iovec {
                        iov_base: (header as *mut Header).cast(),
                        iov_len: size_of::<Header>(),
                    });

                    iovecs.push(libc::iovec {
                        iov_base: part.as_mut_ptr().cast(),
                        iov_len: part.len(),
                    });

                    msghdrs.push(libc::mmsghdr {
                        msg_hdr: libc::msghdr {
                            msg_name: ptr::null_mut(),
                            msg_namelen: 0,
                            msg_iov: iovecs[len..].as_mut_ptr(),
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

            while sent != msghdrs.len() {
                let n = unsafe {
                    libc::sendmmsg(
                        socket.as_raw_fd(),
                        msghdrs[sent..].as_mut_ptr(),
                        msghdrs[sent..].len().try_into()?,
                        libc::MSG_NOSIGNAL.try_into()?,
                    )
                };

                if n == -1 {
                    panic!("sendmmsg error: {:?}", io::Error::last_os_error());
                }

                sent += n as usize;
            }

            headers.clear();
            iovecs.clear();
            msghdrs.clear();

            frame += 1;
        }

        next_frame += frame_time;
        thread::sleep_until(next_frame);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    panic!("not running on a kindle?")
}
