use crate::config::Config;
use std::time::Duration;

pub trait Controller {
    fn new(config: &Config) -> Self;
    fn add(&mut self) -> Duration;
}
