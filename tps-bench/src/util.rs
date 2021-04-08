#[macro_export]
macro_rules! prompt_and_exit {
    ($($arg:tt)*) => ({
        eprintln!($($arg)*);
        log::error!($($arg)*);
        ::std::process::exit(1);
    })
}

pub(crate) fn estimate_fee(outputs_count: u64) -> u64 {
    let min_fee_rate = 1000u64; // shannons/KB
    outputs_count * min_fee_rate
}
