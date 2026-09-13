# M5 (MutationMountOverViolation) 反例 —— SingleSource（§E.4 点不变式）被违反
#
# 复现当前 kernel.rs mount_candidate 缺 capability-overlap guard 的缺陷。
# 本 trace 为模型运行（authority-conformant 守卫被 M5 撤掉 = 当前 Rust 行为）。
#
# C 被 §G.6 违约 latch → 永不离开 Unloading；P1 withdraw 后因 relied guard
# latched 永不可完成移除；mount_candidate 不查 capability 重叠 → P2 挂载 →
# 两个 installed fibers 同时声明提供 K。ADR：§E.4 frozen invariant + §L.2
# 「违约边不再发请求」。

state 1: present=('TRUE', 'FALSE', 'FALSE') | state=('"pending"', '"absent"', '"absent"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '0', '0') | violated=('FALSE', 'FALSE', 'FALSE')
state 2: present=('TRUE', 'TRUE', 'FALSE') | state=('"pending"', '"pending"', '"absent"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '0', '0') | violated=('FALSE', 'FALSE', 'FALSE')
state 3: present=('TRUE', 'TRUE', 'FALSE') | state=('"pending"', '"unloading"', '"absent"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '0', '0') | violated=('FALSE', 'TRUE', 'FALSE')
state 4: present=('TRUE', 'TRUE', 'FALSE') | state=('"pending"', '"unloading"', '"absent"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '1', '0') | violated=('FALSE', 'TRUE', 'FALSE')
state 5: present=('TRUE', 'TRUE', 'TRUE') | state=('"pending"', '"unloading"', '"pending"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '1', '0') | violated=('FALSE', 'TRUE', 'FALSE')
state 6: present=('?',) | state=('?',) | retired=('FALSE', 'FALSE', 'FALSE') | committed=('?',) | effects=('0', '1', '0') | violated=('?',)