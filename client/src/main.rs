#![allow(unused)]
#![feature(push_mut)]

mod framebuffer;

use std::fs::File;
use std::io::{Read, Seek};
use std::net::UdpSocket;
use std::os::unix::prelude::*;
use std::time::Instant;
use std::{array, io, iter, mem, ptr};

use anyhow::bail;
use shared::messages::Header;
use shared::{codec, time};

use crate::framebuffer::Framebuffer;

const DISPLAY_SIZE: usize = 1872 * 2480;
const PACKET_SIZE: usize = 1472;
const THREADS: usize = 2;

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let mut fb0 = File::open("/dev/fb0")?;
        let mut screen: Vec<u8> = vec![0; DISPLAY_SIZE];

        fb0.read_exact(&mut screen)?;

        let [mut output0, mut output1]: [Vec<u8>; THREADS] = [
            Vec::with_capacity(DISPLAY_SIZE / THREADS),
            Vec::with_capacity(DISPLAY_SIZE / THREADS),
        ];

        codec::encode_threaded(&screen, [&mut output0, &mut output1]);

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect("10.0.0.28:9921")?;

        let messages = output0.len().div_ceil(PACKET_SIZE - size_of::<Header>())
            + output1.len().div_ceil(PACKET_SIZE - size_of::<Header>());

        /* If any of these resize, we will segfault! */
        let mut headers: Vec<Header> = Vec::with_capacity(messages);
        let mut iovecs: Vec<libc::iovec> = Vec::with_capacity(messages * 2 + 2);
        let mut msghdrs: Vec<libc::mmsghdr> = Vec::with_capacity(messages + 1);

        let mut a = *b"SIZE";
        let mut b = ((output0.len() + output1.len()) as u32).to_be_bytes();

        iovecs.push(libc::iovec {
            iov_base: a.as_mut_ptr().cast(),
            iov_len: size_of::<[u8; 4]>(),
        });

        iovecs.push(libc::iovec {
            iov_base: b.as_mut_ptr().cast(),
            iov_len: size_of::<[u8; 4]>(),
        });

        msghdrs.push(libc::mmsghdr {
            msg_hdr: libc::msghdr {
                msg_name: ptr::null_mut(),
                msg_namelen: 0,
                msg_iov: iovecs[0..].as_mut_ptr(),
                msg_iovlen: 2,
                msg_control: ptr::null_mut(),
                msg_controllen: 0,
                msg_flags: 0,
            },
            msg_len: 0,
        });

        let mut start: u32 = 0;

        for output in [&mut output0, &mut output1] {
            for chunk in output.chunks_mut(PACKET_SIZE - size_of::<Header>()) {
                let header = headers.push_mut(Header {
                    start: start.to_be_bytes(),
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

                start += chunk.len() as u32;
            }
        }

        let n = unsafe {
            libc::sendmmsg(
                socket.as_raw_fd(),
                msghdrs.as_mut_ptr(),
                msghdrs.len().try_into()?,
                libc::MSG_NOSIGNAL.try_into()?,
            )
        };

        if n == -1 {
            bail!("sendmmsg error: {:?}", io::Error::last_os_error());
        }

        dbg!(n);
    }

    Ok(())
}
