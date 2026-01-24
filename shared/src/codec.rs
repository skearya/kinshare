pub fn encode(frame: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(frame.len() / 2);

    if let Some(mut color) = frame.first().copied() {
        let mut count: u32 = 1;

        for p in frame.iter().cloned().skip(1) {
            if p == color {
                count += 1
            } else {
                output.extend_from_slice(&count.to_be_bytes());
                output.push(color);

                color = p;
                count = 1;
            }
        }

        output.extend_from_slice(&count.to_be_bytes());
        output.push(color);
    }

    output
}

pub fn decode(frame: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(frame.len() * 2);

    let (encodings, []) = frame.as_chunks::<5>() else {
        panic!()
    };

    let encodings = encodings
        .iter()
        .map(|&[count0, count1, count2, count3, color]| {
            (u32::from_be_bytes([count0, count1, count2, count3]), color)
        });

    for (count, color) in encodings {
        for _ in 0..count {
            output.push(color)
        }
    }

    output
}
