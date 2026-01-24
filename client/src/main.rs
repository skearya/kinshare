#![allow(unused)]
#![feature(portable_simd)]

mod framebuffer;

use std::fs::File;
use std::io::Read;
use std::net::UdpSocket;
use std::os::unix::prelude::*;
use std::time::Instant;
use std::{io, mem, ptr};

use anyhow::bail;
use shared::time;

fn main() -> anyhow::Result<()> {
    let mut fb0 = File::open("/dev/fb0")?;

    #[cfg(target_os = "linux")]
    unsafe {
        use core::{mem, slice};

        use libc::munmap;

        use crate::framebuffer::*;

        let mut fixed = std::mem::zeroed::<FixScreeninfo>();

        if libc::ioctl(fb0.as_raw_fd(), FBIOGET_FSCREENINFO.try_into()?, &mut fixed) == -1 {
            bail!("ioctl error: {:?}", io::Error::last_os_error());
        }

        let mut var = std::mem::zeroed::<VarScreeninfo>();

        if libc::ioctl(fb0.as_raw_fd(), FBIOGET_VSCREENINFO.try_into()?, &mut var) == -1 {
            bail!("ioctl error: {:?}", io::Error::last_os_error());
        }

        dbg!(fixed, var);

        let buffer = time!({
            let buffer = libc::mmap(
                ptr::null_mut(),
                1872 * 2480,
                libc::PROT_READ,
                libc::MAP_SHARED,
                fb0.as_raw_fd(),
                0,
            );

            if buffer == libc::MAP_FAILED {
                bail!("mmap error: {:?}", io::Error::last_os_error());
            }

            buffer
        });

        let slice = slice::from_raw_parts_mut(buffer as *mut u8, 1872 * 2480);

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect("10.0.0.28:9921")?;

        time!({
            let mut offset = 0;

            while offset < 1872 * 2480 {
                let n = socket.send(&slice[offset..(offset + 1500).min(1872 * 2480)])?;
                offset += n;
            }
        });

        if munmap(buffer, 1872 * 2480) == -1 {
            bail!("munmap error: {:?}", io::Error::last_os_error());
        }
    }

    Ok(())
}
