
## Usage

```
./target/release/tps-bench bench --spec staging --rpc-urls <ENDPOINTS>
```

* `--spec` could be `staging` or `dev` which included in `specs` directory, or you can create your own configuration file for tps-bench.

* `--rpc-urls` means the target ckb network rpc urls, and you can set multiple urls seperated by spacing like `--rpc-urls rpc_url1 rpc_url2`

---

The other subcommand works like

```
./target/release/tps-bench mine --spec staging --rpc-urls <ENDPOINTS> --blocks block_amount
```

`mine` subcommand will start a miner and generate `block_amount` blocks, if bencher's UTXO set is empty, this would be very helpful.

---

The default data directory (configured via `data_dir`), two files inside this directory:

  * `bench.log`, program logs
  * `metrics.json`, saved the most recent tps

## TODO

  * Support "protocols://username:password@url"
  * Figure out the relation between the average block time and TPS
  * Figure out the relation between the send transaction rate and TPS
  * Make the result be stable
  * Handle lost transaction, transaction may be lost by all nodes
  * Panic hook: panic_on_abort, print exit message
  * Start from `tip - 1000` but not genesis
  * `ckb` monitor `get_block_template` timeused
  * How to find the best TPS
