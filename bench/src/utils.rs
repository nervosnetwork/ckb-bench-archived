use ckb_crypto::secp::Privkey;
use failure::{format_err, Error};
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub fn privkey_from<S: ToString>(privkey_string: S) -> Result<Privkey, Error> {
    let privkey_string = privkey_string.to_string();
    let privkey_str = if privkey_string.starts_with("0x") || privkey_string.starts_with("0X") {
        &privkey_string[2..]
    } else {
        privkey_string.as_str()
    };
    Privkey::from_str(privkey_str.trim()).map_err(|err| format_err!("{}", err))
}

pub fn wait_until<F>(timeout: Duration, mut f: F) -> bool
where
    F: FnMut() -> bool,
{
    let start = Instant::now();
    while !f() && start.elapsed() <= timeout {
        sleep(Duration::new(2, 0));
    }

    start.elapsed() < timeout
}
