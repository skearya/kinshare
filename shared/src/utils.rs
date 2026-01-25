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
