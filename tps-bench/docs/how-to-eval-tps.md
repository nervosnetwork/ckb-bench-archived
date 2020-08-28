# How to evaluate TPS of blockchain

TPS 的定义：

> In the context of blockchains, transactions per second (TPS) refers to the number of transactions that a network is capable of processing each second.

上面的定义很好理解：整个区块链网络作为一个整体，一秒内所能处理的交易量。问题在如何评估 “处理的交易量”？

* 方式一：“处理” 等于 “上链”，所以 `TPS = 已上链的交易量 / 块的时间间隔`

  ```
  TPS = 已上链的交易量 / 块的时间间隔
      = SUM([block[i].transactions for i in start..=end]) / (block[end].timestamp - block[start].timestamp)
  ```

  这种理解方式不全面。因为上链只是区块链处理一个交易的最后一个步骤。压测时观测链上数据可以发现，ckb 链的相邻块的交易数量抖动剧烈，不平滑。

  虽然短期内交易数量剧烈抖动，但拉长评估周期求平均呢？拉长评估周期后，交易的生命周期缩小成点，此时再求平均，好像也是个办法。

  这种方式站在一个全局的视角。

* 方式二：“处理” 覆盖一个交易的整个生命周期，包括进入交易池、同步交易到整个网络、打包、同步块到整个网络，一共四个阶段。

  tps-bench 采用这种覆盖得更全面的理解方式，把 RPC `send_transaction` 开始的时间点视作生命周期的起始，包含该交易的块上链且同步全网视作生命周期的结束（TODO: 补一张图）。

  这种方式站在交易操作者的视角，可能需要关心出块间隔、初始准备的交易数量等变量。

上述两种理解方式分别对应着两种压测方式：

* 方式一：狂发交易。截断链的前面部分和后面部分，`TPS = 已上链的交易量 / 块的时间间隔`。

* 方式二：初始时操作者拥有 `N` 个 UTXO。操作者自己给自己来回转账这 `N` 个 UTXO，交易的频率 `f` 可以换算为 `tps`。假设操作者能持续稳定地每隔 `t` 时长做一次转账，则 `tps = 1s / t`，然后我们只要找到一个稳定的最小 `t` 值即可知道最终 `tps = 1s / t_min`。

---

新的 TPS-Bench 采取的是第一种计算方式，以固定的频率持续发送交易和固定的时间间隔生成 block，从而使得交易池中始终存在充足的备选交易。在经过 warmup 个区块使得对交易的处理趋于稳定后，选取区块链的某一个片段，计算这个片段中处理的交易总数 total_tx，进而可以计算得到平均每个 block 处理的交易数，即为 TPS。

在新的计算方式中存在一些可能影响到 TPS 结果的变量：

  * send_delay: 发送交易的时间间隔，间隔过大，有可能导致交易池中的交易被消费完；间隔过小，有可能导致交易池中积压了过多的交易，占用太多内存。目前采用二分法，在 min = 0， max = 1s / send_delay_0.tps 计算出一个最优的 send_delay 从而获得 TPS
  * block_time: 生成 block 的间隔时间
  * MethodToEvalNetStable: 评估 network 是否开始平稳处理交易的方法
  * MethodToEvalNetStable.warmup 和 MethodToEvalNetStable.window: 计算所需的 block 片段窗口选取时机和窗口大小
  * network: 整个被测试 ckb 的节点状态，网络状况，以及有出现的分叉和交易丢失等情况
  * block 处理交易的数量似乎存在某种以 proposal window 为周期的抖动

针对上述这些因素，后续会进行更多的测试和调整。
