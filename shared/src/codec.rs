use core::simd;
use core::simd::prelude::*;

pub fn encode(frame: &[u8], output: &mut Vec<u8>) {
    let Some(mut color) = frame.first().copied() else {
        return;
    };

    let mut count: u32 = 1;

    for &c in &frame[1..] {
        if c == color {
            count += 1;
        } else {
            output.extend_from_slice(&count.to_be_bytes());
            output.push(color);

            color = c;
            count = 1;
        }
    }

    output.extend_from_slice(&count.to_be_bytes());
    output.push(color);
}

pub fn simdencode(frame: &[u8], output: &mut Vec<u8>) {
    let Some(mut color) = frame.first().copied() else {
        return;
    };

    let mut count: u32 = 1;
    let mut i = 1;

    while i + 16 < frame.len() {
        let data = simd::u8x16::from_slice(&frame[i..]);
        let target = simd::u8x16::splat(color);
        let mask = data.simd_ne(target).to_bitmask();

        if mask == 0 {
            i += 16;
            count += 16;
        } else {
            let at = mask.trailing_zeros();

            output.extend_from_slice(&(count + at).to_be_bytes());
            output.push(color);

            i += at as usize;
            color = frame[i];
            count = 0;
        }
    }

    for &c in &frame[i..] {
        if c == color {
            count += 1;
        } else {
            output.extend_from_slice(&count.to_be_bytes());
            output.push(color);

            color = c;
            count = 1;
        }
    }

    output.extend_from_slice(&count.to_be_bytes());
    output.push(color);
}

pub fn simdencodev0(frame: &[u8], output: &mut Vec<u8>) {
    let Some(mut color) = frame.first().copied() else {
        return;
    };

    let mut count: u32 = 1;
    let mut i = 1;

    while i + 16 < frame.len() {
        let data = simd::u8x16::from_slice(&frame[i..]);
        let target = simd::u8x16::splat(color);
        let first_ne = data.simd_ne(target).first_set();

        match first_ne {
            None => {
                i += 16;
                count += 16;
            }
            Some(at) => {
                output.extend_from_slice(&(count + at as u32).to_be_bytes());
                output.push(color);

                i += at + 1;
                color = frame[i - 1];
                count = 1;
            }
        }
    }

    for &c in &frame[i..] {
        if c == color {
            count += 1;
        } else {
            output.extend_from_slice(&count.to_be_bytes());
            output.push(color);

            color = c;
            count = 1;
        }
    }

    output.extend_from_slice(&count.to_be_bytes());
    output.push(color);
}

pub fn decode(frame: &[u8], output: &mut Vec<u8>) {
    let (encodings, []) = frame.as_chunks::<5>() else {
        return;
    };

    let encodings = encodings
        .iter()
        .map(|&[count0, count1, count2, count3, color]| {
            (u32::from_be_bytes([count0, count1, count2, count3]), color)
        });

    for (count, color) in encodings {
        for _ in 0..count {
            output.push(color);
        }
    }
}
