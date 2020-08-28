# Configurations

### 配置

* `data_dir`

  主要存放两个文件：

    - `bench.log`: 所有 tps-bench 程序执行的日志
    - `metrics.json`: 写入 tps benchmark 的最终结果

* `bencher_private_key`

  Capacities provider 的私钥

  做普通的压测时，将其配置为目标 ckb 的 `block_assembler` 一个账户。

* `private_key`

  miner 的 私钥，当 miner 与 bencher 为不同账户时，在进行压测时会将 miner 的余额转账给 bencher 用于生成交易

* `block_time`

  出块间隔，单位为 `ms`。
  因为程序里面集成了出块逻辑，所以需要指定出块间隔。
  做普通的压测时，将其配置为 `1000` 或 `8000`。

* `transaction_type`

  压测的交易类型，可选 `"In1Out1"`, `"In2Out2"`, `"In3Out3"`

* `send_delay`

  发送交易的间隔时间，单位为 `ms`。
  由于出块的间隔时间和交易的处理能力限制，发送交易的间隔时间并非越短越好，当积压交易达到一定程度时，交易池会占用很大的内存空间，导致交易的处理能力反向收到影响，因此在计算 tps 时，除了在配置文件中指定的 send_delay 外，会额外计算一个最优的 tps 值。
