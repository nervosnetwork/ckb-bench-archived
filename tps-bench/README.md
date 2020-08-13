
## Usage

```
./target/release/tps-bench bench --spec staging --rpc-urls <ENDPOINTS> --seconds 1000
```

The default data directory (configured via `data_dir`), two files inside this directory:
    * `bench.log`, program logs
    * `metrics.json`, saved the most recent tps

## TODO

  * Configured benchmark => TPS
  * Support multiple endpoints
  * Support "protocols://username:password@url"
  * Figure out the relation between the average block time and TPS
  * Figure out the relation between the send transaction rate and TPS
  * Make the result be stable
  * Handle lost transaction, transaction may be lost by all nodes
  * Based benchmark
  * Adjust send rate
