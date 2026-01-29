#![allow(unused)]
#![feature(push_mut)]

mod framebuffer;

use std::fs::File;
use std::io::{Read, Seek};
use std::net::UdpSocket;
use std::ops::Range;
use std::os::unix::prelude::*;
use std::time::{Duration, Instant};
use std::{array, io, iter, mem, ptr, thread};

use anyhow::bail;
use shared::messages::Header;
use shared::{codec, time};

use crate::framebuffer::Framebuffer;

const DISPLAY_SIZE: usize = 1872 * 2480;
const PACKET_SIZE: usize = 1472;
const THREADS: usize = 2;

#[cfg(target_os = "linux")]
fn main() -> anyhow::Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("192.168.15.245:9921")?;

    let mut fb0 = File::open("/dev/fb0")?;

    // Reused screen buffers that each thread uses.
    let [mut screen0, mut screen1]: [Vec<u8>; THREADS] = [
        vec![0; DISPLAY_SIZE / THREADS],
        vec![0; DISPLAY_SIZE / THREADS],
    ];

    // Reused encoded output buffers that each thread uses.
    let [mut output0, mut output1]: [Vec<u8>; THREADS] = [
        Vec::with_capacity(DISPLAY_SIZE / THREADS),
        Vec::with_capacity(DISPLAY_SIZE / THREADS),
    ];

    // Reused buffers for sendmmsg().
    // If any of these resize unexpectedly, we will segfault!
    let mut headers: Vec<Header> = vec![];
    let mut iovecs: Vec<libc::iovec> = vec![];
    let mut msghdrs: Vec<libc::mmsghdr> = vec![];
    let mut frame: u32 = 1;

    loop {
        codec::encode_threaded_with_read(
            &fb0,
            DISPLAY_SIZE,
            [&mut screen0, &mut screen1],
            [&mut output0, &mut output1],
        );

        let messages = [&output0, &output1]
            .iter()
            .map(|output| output.len().div_ceil(PACKET_SIZE - size_of::<Header>()))
            .sum::<usize>();

        let size = [&output0, &output1]
            .iter()
            .map(|output| output.len() as u32)
            .sum::<u32>();

        headers.reserve_exact(messages.saturating_sub(headers.len()));
        iovecs.reserve_exact((messages * 2).saturating_sub(iovecs.len()));
        msghdrs.reserve_exact(messages.saturating_sub(msghdrs.len()));

        let mut offset: u32 = 0;

        for output in [&mut output0, &mut output1] {
            for chunk in output.chunks_mut(PACKET_SIZE - size_of::<Header>()) {
                let header = headers.push_mut(Header {
                    frame: frame.to_be_bytes(),
                    size: size.to_be_bytes(),
                    offset: offset.to_be_bytes(),
                    length: (chunk.len() as u16).to_be_bytes(),
                });

                let len = iovecs.len();

                iovecs.push(libc::iovec {
                    iov_base: header as *mut Header as *mut libc::c_void,
                    iov_len: size_of::<Header>(),
                });

                iovecs.push(libc::iovec {
                    iov_base: chunk.as_mut_ptr().cast(),
                    iov_len: chunk.len(),
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

                offset += chunk.len() as u32;
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
                bail!("sendmmsg error: {:?}", io::Error::last_os_error());
            }

            sent += n as usize;
        }

        output0.clear();
        output1.clear();

        headers.clear();
        iovecs.clear();
        msghdrs.clear();

        frame += 1;

        thread::sleep(Duration::from_millis(12));
    }

    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn main() {
    panic!("not running on a kindle?")
}
