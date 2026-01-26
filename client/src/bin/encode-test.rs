use std::fs;
use std::time::Duration;

const RUNS: u128 = 100;

fn main() -> anyhow::Result<()> {
    let frame = fs::read("raw/frame.raw")?;
    let mut output = Vec::with_capacity(frame.len());

    let mut sum0 = 0;

    for _ in 0..RUNS {
        let start = std::time::Instant::now();
        shared::codec::encode(&frame, &mut output);
        sum0 += start.elapsed().as_nanos();

        output.clear();
    }

    let mut sum1 = 0;

    for _ in 0..RUNS {
        let start = std::time::Instant::now();
        shared::codec::encode_simd(&frame, &mut output);
        sum1 += start.elapsed().as_nanos();

        output.clear();
    }

    println!(
        "regular ({RUNS} runs): {:#?}",
        Duration::from_nanos((sum0 / RUNS).try_into().unwrap()),
    );

    println!(
        "simd ({RUNS} runs): {:#?}",
        Duration::from_nanos((sum1 / RUNS).try_into().unwrap())
    );

    Ok(())
}
