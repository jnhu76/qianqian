# M5 (MutationMountOverViolation) 反例 —— SingleSource（§E.4 点不变式）被违反
#
# 归档 pre-#126 kernel.rs mount_candidate 缺 capability-overlap guard 的
# 历史缺陷行为（已由 #126 修复；baseline 模型守卫在位 = 当前 Rust）。
# 本 trace 为模型运行（现行守卫被 M5 刻意撤掉 = pre-#126 Rust 行为）。
#
# 场景（provider 自身违约变体）：P1 激活期 §G.6 违约 latch（state 3–4），
# 永不可移除；desired P1→P2 提交后 mount 不查 capability 重叠 → P2 于
# state 5 挂载 → 两个 installed fibers 同时声明提供 K。
# （provenance：2026-09-13 FV-TEMP-0 轮的模型运行记录；trace 步形属当轮
# 模型修订，语义与本仓现行模型等价——违约 latch + overlap 挂载。）

state 1: present=('TRUE', 'FALSE', 'FALSE') | state=('"pending"', '"absent"', '"absent"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '0', '0') | violated=('FALSE', 'FALSE', 'FALSE')
state 2: present=('TRUE', 'TRUE', 'FALSE') | state=('"pending"', '"pending"', '"absent"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '0', '0') | violated=('FALSE', 'FALSE', 'FALSE')
state 3: present=('TRUE', 'TRUE', 'FALSE') | state=('"pending"', '"unloading"', '"absent"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '0', '0') | violated=('FALSE', 'TRUE', 'FALSE')
state 4: present=('TRUE', 'TRUE', 'FALSE') | state=('"pending"', '"unloading"', '"absent"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '1', '0') | violated=('FALSE', 'TRUE', 'FALSE')
state 5: present=('TRUE', 'TRUE', 'TRUE') | state=('"pending"', '"unloading"', '"pending"') | retired=('FALSE', 'FALSE', 'FALSE') | committed=('NoOne', 'NoOne', 'NoOne') | effects=('0', '1', '0') | violated=('FALSE', 'TRUE', 'FALSE')
state 6: present=('?',) | state=('?',) | retired=('FALSE', 'FALSE', 'FALSE') | committed=('?',) | effects=('0', '1', '0') | violated=('?',)