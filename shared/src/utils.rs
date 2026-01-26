#[macro_export]
macro_rules! time {
    ($b:block) => {{
        let start = std::time::Instant::now();
        let result = $b;
        let duration = start.elapsed();

        dbg!(duration);

        result
    }};
    ($e:literal, $b:block) => {{
        let start = std::time::Instant::now();
        let result = $b;
        let duration = start.elapsed();

        eprintln!(
            "[{}:{}:{}] {} = {:#?}",
            file!(),
            line!(),
            column!(),
            $e,
            duration
        );

        result
    }};
}

#[macro_export]
macro_rules! quickbench {
    ($n:literal, $name:literal, $run:block, $cleanup:block) => {{
        let sum = iter::repeat_with(|| {
            let start = std::time::Instant::now();
            $run;
            let elapsed = start.elapsed().as_nanos();
            $cleanup;

            elapsed
        })
        .take($n)
        .sum::<u128>();

        let avg = std::time::Duration::from_nanos_u128(sum / $n);

        eprintln!(
            "[{}:{}:{}] {} runs - {} = {:#?}",
            file!(),
            line!(),
            column!(),
            $n,
            $name,
            avg
        );
    }};
}
