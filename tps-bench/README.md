
## Usage

```
./target/release/tps-bench -c config.toml bench
```

The default data directory (configured via `data_dir`) is `./tpsbench`, two files inside this directory:
    * `bench.log`, program logs
    * `metrics.json`, saved the most recent tps

## TODO

  * Default config.toml, and support command option `--rpc-urls`
  * Support "protocols://username:password@url"
  * Output average block time
  * The relation between the average block time and TPS
  * The relation between the send transaction rate and TPS
