#![allow(unused)]

mod framebuffer;

use std::fs::File;
use std::io::Read;
use std::net::UdpSocket;
use std::os::unix::prelude::*;
use std::time::Instant;
use std::{io, mem, ptr};

use anyhow::bail;
use shared::time;

use crate::framebuffer::Framebuffer;

const PACKET_SIZE: usize = 1472;

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let framebuffer = unsafe { Framebuffer::new() }?;

        let socket = UdpSocket::bind("0.0.0.0:0")?;
        socket.connect("0.0.0.0:9921")?;

        time!({
            let mut offset = 0;

            while offset < 1872 * 2480 {
                let n =
                    socket.send(&framebuffer[offset..(offset + PACKET_SIZE).min(1872 * 2480)])?;
                offset += n;
            }
        });
    }

    Ok(())
}
