# Configurations

### 配置

* `private_key`

  Capacities provider 的私钥

  做普通的压测时，将其配置为目标 ckb 的 `block_assembler` 一个账户。

* `node_urls`

  目标 ckb 的 rpc 地址列表

* `block_time`

  出块间隔，单位为 `ms`。
  因为程序里面集成了出块逻辑，所以需要指定出块间隔。
  做普通的压测时，将其配置为 `1000` 或 `8000`。

* `transaction_type`

  压测的交易类型，可选 `"In1Out1"`, `"In2Out2"`, `"In3Out3"`

* `transaction_count`

  压测要准备的交易数量。原则上这个数值应该硬编码的，但是现在不知道应该硬编码为多少，所以先作为配置项。
