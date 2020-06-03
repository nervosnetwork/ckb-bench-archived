#![macro_use]

#[macro_export]
macro_rules! prompt_and_exit {
    ($($arg:tt)*) => ({
        eprintln!($($arg)*);
        ::std::process::exit(1);
    })
}
