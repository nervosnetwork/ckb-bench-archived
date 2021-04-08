use crate::config::{Config, Spec, Url};
use crate::prompt_and_exit;

pub const MINE_SUBCOMMAND: &str = "mine";
pub const BENCH_SUBCOMMAND: &str = "bench";
pub const METRICS_SUBCOMMAND: &str = "metric";

#[derive(Debug, Clone)]
pub enum CommandLine {
    MineMode(Config, u64 /* blocks */),
    BenchMode(Config, bool),
    MetricMode(Vec<Url>),
}

pub fn commandline() -> CommandLine {
    include_str!("../Cargo.toml");
    let matches = clap::app_from_crate!()
        .subcommand(
            clap::SubCommand::with_name(MINE_SUBCOMMAND)
                .about(
                    "Start miner and exit after generating corresponding blocks\n\
                     Example:\n\
                     tps-bench mine -s dev --rpc-urls http://127.0.0.1:8114 -b 100",
                )
                .arg(clap::Arg::from_usage(
                    "-s, --spec <FILE> 'the spec: staging, dev, release or path to spec file'",
                ))
                .arg(
                    clap::Arg::from_usage("--rpc-urls <ENDPOINTS> 'the ckb rpc endpoints'")
                        .required(true)
                        .multiple(true)
                        .validator(|s| Url::parse(&s).map(|_| ()).map_err(|err| err.to_string())),
                )
                .arg(
                    clap::Arg::from_usage(
                        "-b --blocks <NUMBER> 'the number of blocks to generate'",
                    )
                    .required(true)
                    .validator(|s| s.parse::<u64>().map(|_| ()).map_err(|err| err.to_string())),
                ),
        )
        .subcommand(
            clap::SubCommand::with_name(BENCH_SUBCOMMAND)
                .about(
                    "Run TPS bencher and caculate tps-bench in a specify blocks window\n\
                    Example:\n\
                    tps-bench bench -s dev --rpc-urls http://127.0.0.1:8114",
                )
                .arg(clap::Arg::from_usage(
                    "-s, --spec <FILE> 'the spec: staging, dev, release or path to spec file'",
                ))
                .arg(
                    clap::Arg::from_usage("--rpc-urls <ENDPOINTS> 'the ckb rpc endpoints'")
                        .required(true)
                        .multiple(true)
                        .validator(|s| Url::parse(&s).map(|_| ()).map_err(|err| err.to_string())),
                )
                .arg(clap::Arg::from_usage(
                    "[skip-best-tps-caculation] --skip-best-tps-caculation 'run bench with skip best tps caculation'",
                )),
        )
        .subcommand(
            clap::SubCommand::with_name(METRICS_SUBCOMMAND)
                .about("Caculate tps metrics in a specify blocks window")
                .arg(
                    clap::Arg::from_usage("--rpc-urls <ENDPOINTS> 'the ckb rpc endpoints'")
                        .required(true)
                        .multiple(true)
                        .validator(|s| Url::parse(&s).map(|_| ()).map_err(|err| err.to_string())),
                ),
        )
        .get_matches();

    match matches.subcommand() {
        (MINE_SUBCOMMAND, Some(options)) => {
            let spec = {
                let filepath = options
                    .value_of("spec")
                    .expect("clap arg option `required(true)` checked");
                match Spec::load(filepath) {
                    Ok(spec) => spec,
                    Err(err) => prompt_and_exit!("Spec::load({}) error: {:?}", filepath, err),
                }
            };
            let rpc_urls = options
                .values_of("rpc-urls")
                .expect("clap arg option `required(true)` checked")
                .map(|str| Url::parse(str).expect("clap arg option `validator` checked"))
                .collect::<Vec<_>>();
            let config = Config::new(spec, rpc_urls);
            let blocks = options
                .value_of("blocks")
                .expect("clap arg option `required(true)` checked")
                .parse::<u64>()
                .expect("clap arg option `validator` checked");
            CommandLine::MineMode(config, blocks)
        }
        (BENCH_SUBCOMMAND, Some(options)) => {
            let spec = {
                let filepath = options
                    .value_of("spec")
                    .expect("clap arg option `required(true)` checked");
                match Spec::load(filepath) {
                    Ok(spec) => spec,
                    Err(err) => prompt_and_exit!("Spec::load({}) error: {:?}", filepath, err),
                }
            };
            let rpc_urls = options
                .values_of("rpc-urls")
                .expect("clap arg option `required(true)` checked")
                .map(|str| Url::parse(str).expect("clap arg option `validator` checked"))
                .collect::<Vec<_>>();
            let skip_best_tps_caculation = options.is_present("skip-best-tps-caculation");
            let config = Config::new(spec, rpc_urls);
            CommandLine::BenchMode(config, skip_best_tps_caculation)
        }
        (METRICS_SUBCOMMAND, Some(options)) => {
            let rpc_urls = options
                .values_of("rpc-urls")
                .expect("clap arg option `required(true)` checked")
                .map(|str| Url::parse(str).expect("clap arg option `validator` checked"))
                .collect::<Vec<_>>();
            CommandLine::MetricMode(rpc_urls)
        }
        (subcommand, options) => {
            prompt_and_exit!(
                "unsupported subcommand: `{}`, options: {:?}",
                subcommand,
                options
            );
        }
    }
}
