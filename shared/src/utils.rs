#[macro_export]
macro_rules! time {
    ($b:block) => {{
        let start = Instant::now();
        let result = $b;
        let duration = start.elapsed();

        dbg!(duration);

        result
    }};
}
