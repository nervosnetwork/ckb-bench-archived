use crate::config::Config;
use crate::controller::Controller;
use crate::BLOCK_TIME;
use std::time::{Duration, Instant};

// We adjust the sleep time every time after sending
// ` ADJUST_SLEEP_TIME_WHEN_SENT_ROUNDS * transaction_count` transactions
const ADJUST_SLEEP_TIME_WHEN_SENT_ROUNDS: usize = 2;

// TODO
const ERROR_MARGIN: Duration = Duration::from_millis(100);

// Control strategy: Expect the interval among the sent transactions are nearly equal to `sleep`.
// Increase the `sleep` when there is an interval greater than the `sleep`, otherwise reduce.
pub struct LocalController {
    config: Config,
    sent: Vec<Instant>,
    max_sleep_time: Duration,
    min_sleep_time: Duration,
}

impl Controller for LocalController {
    fn new(config: &Config) -> Self {
        Self {
            config: config.clone(),
            sent: Vec::new(),
            max_sleep_time: Duration::from_millis(500),
            min_sleep_time: Duration::from_secs(0),
        }
    }

    fn add(&mut self) -> Duration {
        self.sent.push(Instant::now());

        if self.sent.len()
            == self.config.transaction_count as usize * ADJUST_SLEEP_TIME_WHEN_SENT_ROUNDS
        {
            self.adjust_sleep_time();
        }

        self.sleep_time()
    }
}

impl LocalController {
    fn sleep_time(&self) -> Duration {
        (self.max_sleep_time + self.min_sleep_time) / 2
    }

    fn tps(&self) -> f64 {
        Duration::from_secs(1).as_secs_f64() / self.sleep_time().as_secs_f64()
    }

    fn adjust_sleep_time(&mut self) {
        let sleep_time = self.sleep_time();
        let sent_count = self.sent.len() as u64;
        assert!(sent_count > self.config.transaction_count);

        let skip_first_n =
            (ADJUST_SLEEP_TIME_WHEN_SENT_ROUNDS / 2) * self.config.transaction_count as usize;
        let mut margin = Duration::from_secs(0);
        for i in skip_first_n..self.sent.len() - 1 {
            assert!(self.sent[i as usize + 1] >= self.sent[i as usize]);
            let interval = self.sent[i as usize + 1] - self.sent[i as usize];

            assert!(interval >= sleep_time);
            if interval - sleep_time > Duration::from_millis(1) {
                margin += interval - sleep_time - Duration::from_millis(1);
            }
        }

        self.sent = Vec::new();
        if margin <= ERROR_MARGIN {
            self.min_sleep_time = sleep_time;
        } else {
            self.max_sleep_time = sleep_time;
        }

        self.print_table();
        self.wait_transactions_committed();
    }

    fn print_table(&self) {
        let tps = self.tps();
        println!(
            "max_sleep_time: {:?}, min_sleep_time: {:?}, tps: {}",
            self.max_sleep_time, self.min_sleep_time, tps
        );
    }

    // FIXME This function is dirty.
    fn wait_transactions_committed(&self) {
        ::std::thread::sleep(BLOCK_TIME * 10)
    }
}
