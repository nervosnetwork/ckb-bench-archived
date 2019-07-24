use crate::miner::MinerConfig;
use clap::{App, Arg, SubCommand};
use failure::{format_err, Error};
use serde_derive::{Deserialize, Serialize};
use std::clone::Clone;
use std::collections::HashMap;
use std::fs::{create_dir_all, read_to_string};
use std::ops::{Deref, Range};
use std::path::PathBuf;
use std::time::Duration;

#[derive(PartialEq)]
pub enum Command {
    Bench,
    Mine(u64),
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub basedir: PathBuf,
    pub safe_window: u64,
    pub proposal_window: u64,
    pub logger: ckb_logger::Config,
    pub bank: String,
    pub alice: String,
    pub rpc_urls: Vec<Url>,
    pub serial: Serial,
    #[serde(rename = "miners")]
    pub miner_configs: Vec<MinerConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Serial {
    pub conditions: String,
    pub transactions: usize,
    pub adjust_cycle: usize,
    pub adjust_origin: Duration,
    pub adjust_step: Duration,
    pub adjust_misbehavior: usize,
}

impl Serial {
    pub fn parse_conditions(&self) -> Result<HashMap<Condition, usize>, Error> {
        let c = serde_json::from_str(self.conditions.as_str())?;
        Ok(c)
    }

    pub fn conditions(&self) -> HashMap<Condition, Range<usize>> {
        let conditions = self.parse_conditions().expect("check when loads config");
        let mut sum = 0;
        conditions
            .into_iter()
            .map(|(condition, sg)| {
                sum += sg;
                (condition, sum - sg..sum)
            })
            .collect()
    }
}

#[derive(Copy, Deserialize, Serialize, Debug, Clone, Hash, PartialEq, Eq)]
pub enum Condition {
    In2Out2,
    RandomFee,
    Unresolvable,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Url(#[serde(with = "url_serde")] pub url::Url);

impl Deref for Url {
    type Target = url::Url;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Config {
    pub fn load(path: &str) -> Result<Self, Error> {
        let content = read_to_string(path)?;
        let config: Self = toml::from_str(&content).map_err(|err| format_err!("{}", err))?;

        {
            let mut log_dir = config.basedir.clone();
            log_dir.push("logs");
            create_dir_all(log_dir)?;
        }

        config.serial.parse_conditions()?;

        if config.rpc_urls.is_empty() {
            return Err(format_err!("ckb_nodes is empty"));
        }

        Ok(config)
    }
}

pub fn setup() -> Result<(Command, Config), Error> {
    let matches = App::new("ckb-bench")
        .version("0.1")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .default_value("config.toml")
                .help("Sets a custom config file"),
        )
        .subcommand(
            SubCommand::with_name("mine").about("Mine mode").arg(
                Arg::with_name("blocks")
                    .long("blocks")
                    .value_name("NUMBER")
                    .default_value("10000000000")
                    .help("Set target count of blocks you wanna mine"),
            ),
        )
        .subcommand(SubCommand::with_name("bench").about("bench mode"))
        .get_matches();

    let config = Config::load(matches.value_of("config").unwrap())?;
    match matches.subcommand() {
        ("mine", Some(cmd)) => {
            let blocks = cmd
                .value_of("blocks")
                .unwrap()
                .parse::<u64>()
                .expect("blocks in number");
            Ok((Command::Mine(blocks), config))
        }
        ("mine", None) => unreachable!(),
        ("bench", _) => Ok((Command::Bench, config)),
        ("", _) => Ok((Command::Bench, config)),
        (cmd, _) => Err(format_err!("Not supported subcommand: {}", cmd)),
    }
}
