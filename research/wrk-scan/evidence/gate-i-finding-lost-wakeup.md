# Gate I 实施记录 — fence 负控制暴露并修复的真实 Lost-Wakeup 形状缺陷

日期：2026-10-21。性质：H0 实施过程中由确定性交错控制（test-only fence）暴露、
并在同一轮修复的设计缺陷记录。这是负控制基础设施按预期工作的证据，
不是产品缺陷（research-only 代码），也不是冻结协议变更。

## 现象

`producer_done_with_queue_nonempty_still_drains` 等基于 fence 的确定性交错控制
以约 7/8 的概率挂死（watchdog 超时）。逐步 trace 定位到确切交错：

1. worker 在 `while !pred` 循环体内的 fence 点停车（fence 临时释放 state 锁）；
2. producer 入队全部 item 并 `notify_one/all(work_cv)` —— worker 不在 condvar 上，
   通知全部丢失；
3. 测试打开 fence —— worker 从 fence 返回，**无条件进入 `work_cv.wait`**
   （谓词只在循环顶部复检）→ 永久睡眠。

## 根因

H0 原始循环形状：

```rust
while pred {
    fence_point();          // test-only：临时释放锁
    cv.wait(guard);         // 谓词未复检
}
```

生产构建中 fence 点不存在，谓词检查到 wait 进入之间锁始终被持有，
通知不可能插入——形状是**偶然安全**（依赖锁不释放），而非**结构安全**
（每次锁释放点之后显式复检谓词）。fence 恰好构造了一个真实合法交错
无法产生、但锁释放窗口等价的调度，暴露了该形状对"通知发生在条件检查
之后、正式进入等待之前"这一任务书点名的 Lost-Wakeup 问题不设防。

## 修复

在两个谓词等待循环（worker 的 work_cv、settlement 的 settle_cv）的 fence
返回之后显式复检谓词，不满足才进入 `condvar.wait`：

```rust
while pred {
    fence_point();              // test-only
    if !pred { /* unreachable in non-test builds */ } else { recheck; }
    if pred_recheck_fails { break; }   // notify landed while parked
    cv.wait(guard);
}
```

非测试构建中 fence 为空操作，复检为冗余但无害（每次迭代一次布尔求值）；
测试构建中它使"notify-before-wait"交错**结构上不可能丢失通知**。
修复后：worker 侧与 settlement 侧两个 lost-wakeup 控制确定性 GREEN；
全套 20 测试 8× 重复运行全部 rc=0。

## 对 Gate I 证据的意义

- 负控制基础设施（确定性 fence + watchdog-as-timeout-protection）证明了
  自身的判别力：它能抓住真实交错缺陷，不是摆设。
- 修复后的 `while pred { fence; recheck-pred; wait }` 形状即为
  "通知发生在条件检查之后、正式进入等待之前 ⇒ 不丢通知"的结构性证明
  的实现载体；M-class 单检查变体（`if` 替代 `while`/删除复检）将由
  mutation 臂给出 RED 证据。
- 全部过程日志归档于 `evidence/logs/phase23-*`（含每轮失败现场）。
