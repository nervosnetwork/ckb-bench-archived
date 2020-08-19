
## Usage

```
./target/release/tps-bench bench --spec staging --rpc-urls <ENDPOINTS>
```

The default data directory (configured via `data_dir`), two files inside this directory:
    * `bench.log`, program logs
    * `metrics.json`, saved the most recent tps

## TODO

  * Configured benchmark => TPS
  * Support "protocols://username:password@url"
  * Figure out the relation between the average block time and TPS
  * Figure out the relation between the send transaction rate and TPS
  * Make the result be stable
  * Handle lost transaction, transaction may be lost by all nodes
  * Based benchmark => standard output results(txtype, metrics..)
  * Adjust send rate
  * Async send transactions
  * Update logs redability
  * Truncate the target nodes via RPC [`truncate`](https://github.com/nervosnetwork/ckb/pull/2064) before and after benching
  * Panic hook: panic_on_abort, print exit message
  * Start from `tip - 1000` but not genesis
  * `ckb` monitor `get_block_template` timeused
