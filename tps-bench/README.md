
## Usage

```
./target/release/tps-bench -c config.toml bench
```

The default data directory (configured via `data_dir`) is `./tpsbench`, two files inside this directory:
    * `bench.log`, program logs
    * `metrics.json`, saved the most recent tps

## TODO

  * Support "protocols://username:password@url"
  * The relation between the average block time and TPS
  * The relation between the send transaction rate and TPS
