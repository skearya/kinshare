#![allow(unused)]

mod framebuffer;

use std::fs::File;
use std::io::Read;
use std::net::UdpSocket;
use std::os::unix::prelude::*;
use std::time::Instant;
use std::{array, io, iter, mem, ptr};

use anyhow::bail;
use shared::{codec, time};

use crate::framebuffer::Framebuffer;

const DISPLAY_SIZE: usize = 1872 * 2480;
const PACKET_SIZE: usize = 1472;
const THREADS: usize = 2;

#[repr(C, packed)]
struct Header {
    start: u32,
    length: u16,
}

fn main() -> anyhow::Result<()> {
    // #[cfg(target_os = "linux")]
    {
        let mut fb0 = File::open("/dev/fb0")?;

        let mut buffer: Vec<u8> = vec![0; DISPLAY_SIZE];

        let [mut output0, mut output1]: [Vec<u8>; THREADS] =
            array::from_fn(|i| Vec::with_capacity(DISPLAY_SIZE / THREADS));

        time!("eager copy + read", {
            fb0.read_exact(&mut buffer)?;
            codec::encode_threaded(&buffer, [&mut output0, &mut output1]);
        });

        output0.clear();
        output1.clear();

        let framebuffer = Framebuffer::new()?;

        time!("no-copy mmap", {
            codec::encode_threaded(&framebuffer, [&mut output0, &mut output1]);
        });

        // let framebuffer = Framebuffer::new()?;

        // let [mut output0, mut output1]: [Vec<u8>; THREADS] =
        //     array::from_fn(|i| Vec::with_capacity(DISPLAY_SIZE / THREADS));

        // codec::encode_threaded(&framebuffer, [&mut output0, &mut output1]);

        // let socket = UdpSocket::bind("0.0.0.0:0")?;
        // socket.connect("10.0.0.28:9921")?;

        // let messages = output0.len().div_ceil(PACKET_SIZE - size_of::<Header>())
        //     + output1.len().div_ceil(PACKET_SIZE - size_of::<Header>());

        // let mut headers: Vec<Header> = Vec::with_capacity(messages);
        // let mut iovecs: Vec<libc::iovec> = Vec::with_capacity(messages * 2);
        // let mut msghdrs: Vec<libc::mmsghdr> = Vec::with_capacity(messages);

        // let mut start = 0;

        // for output in [&mut output0, &mut output1] {
        //     for chunk in output.chunks_mut(PACKET_SIZE - size_of::<Header>()) {
        //         headers.push(Header {
        //             start,
        //             length: chunk.len() as u16,
        //         });

        //         let len = iovecs.len();

        //         iovecs.push(libc::iovec {
        //             iov_base: headers.last_mut().unwrap() as *mut Header as *mut libc::c_void,
        //             iov_len: size_of::<Header>(),
        //         });

        //         iovecs.push(libc::iovec {
        //             iov_base: chunk.as_mut_ptr().cast(),
        //             iov_len: chunk.len(),
        //         });

        //         msghdrs.push(libc::mmsghdr {
        //             msg_hdr: libc::msghdr {
        //                 msg_name: ptr::null_mut(),
        //                 msg_namelen: 0,
        //                 msg_iov: &mut iovecs[len] as *mut libc::iovec,
        //                 msg_iovlen: 2,
        //                 msg_control: ptr::null_mut(),
        //                 msg_controllen: 0,
        //                 msg_flags: 0,
        //             },
        //             msg_len: 0,
        //         });

        //         start += chunk.len() as u32;
        //     }
        // }

        // let n = unsafe {
        //     libc::sendmmsg(
        //         socket.as_raw_fd(),
        //         msghdrs.as_mut_ptr(),
        //         msghdrs.len().try_into()?,
        //         libc::MSG_NOSIGNAL.try_into()?,
        //     )
        // };

        // if n == -1 {
        //     bail!("sendmmsg error: {:?}", io::Error::last_os_error());
        // }

        // dbg!(n);
    }

    Ok(())
}
