# 随机化方案的实测证据

`../randomization_design.md`（v0.6）里的每个数字都出自这四个脚本，或出自一次性的 Rust 探针。
本目录的作用是让评审能自己复跑，而不是相信文档里的数字。

脚本都只用标准库，统计部分直接复用
`.claude/skills/animal-grouping/scripts/grouping_engine.py`（零依赖精确参考实现），
所以它们与 Rust 引擎是两套独立实现，可以互相印证。

## 复跑

必须从仓库根目录运行（脚本按相对路径找参考实现和数据）：

```bash
python3 docs/randomization_evidence/manual_workflow.py   # 手工流程 vs 区组随机化，约 9 秒
python3 docs/randomization_evidence/is_it_random.py      # 现有实现是不是随机的，约 9 秒
python3 docs/randomization_evidence/real_accept.py       # 真实数据达标率，约 2 秒
python3 docs/randomization_evidence/accept_rate.py       # 合成数据敏感性，约 3 秒
```

## 四个脚本各自回答什么

| 脚本 | 数据 | 回答的问题 | 对应文档章节 |
| --- | --- | --- | --- |
| `manual_workflow.py` | `src-tauri/tests/fixtures/randomization_input_60f.xlsx` | 实验室的“完全随机”和“分层随机”是不是同一件事？按体重区组能把主指标均衡到什么程度？四种随机化变体各自的差别？靠换随机数重抽要抽多少次才追得上？ | §6.2–§6.4（A/B 等价性）、§7.4（方案 D 对照）、§7.7（四变体）、附录 A.7–A.9 |
| `is_it_random.py` | 同上 | **现有已实现的版本是不是“分层随机”？** 把引擎输出的 min(P) 放到 10 万次随机分配的分布里看它落在哪个位置 | §4.3 |
| `real_accept.py` | 同上（60 只，全雌，2 指标） | 随机分配的达标率是多少？拒绝采样要抽几次？哪个指标是瓶颈？随机分组的 min(P) 分布长什么样？ | §5.3、§5.4（随机分布）、附录 A.3 |
| `accept_rate.py` | 合成数据（36 只 3 组 × 12） | 达标率受指标**变异**影响还是受指标**个数**影响？ | §5.3 的衰减表 |

`real_accept.py` 的 `LAYOUTS` 常量列出了四种分组布局加一种带备用组的布局，
换布局只需改这个常量。`accept_rate.py` 的 `synth()` 里有四种数据生成方式
（正态、均匀、对数正态、双峰），用来验证结论对分布形状不敏感。

`manual_workflow.py` 内置两个独立的 2000 次抽样批次：A/B/C 三种流程对比段与
四种随机化变体对比段。设计文档 §7.4 引用前者、§7.7 引用后者，两处个别分位数的
差异（如 6 组 q95 0.345 / 0.330）是独立批次的抽样波动，不是错误。

## 预期输出的关键数字

### 手工流程 vs 区组随机化（`manual_workflow.py`，6 组 × 10）

```
procedure                        BW P median  BW pass  both pass  BW mean spread (g)
A  sequential fill (完全随机)          0.491    92.9%      85.0%  med 1.215  max 2.715
B  cyclic fill     (分层随机)          0.506    94.2%      87.8%  med 1.215  max 2.865
C  blocked by BW   (proposed)         1.000   100.0%      90.8%  med 0.225  max 0.465

A vs B: KS statistic 0.0088 (critical value ~0.0215)  -> same randomization
C + CD45 criterion: 500/500 runs accepted, mean 1.11 draws, max 3
matching C's balance by re-drawing under A: 1.59% per draw -> 63 draws expected
```

三个数字是全套论证的支点：A 与 B 的 KS 统计量远低于临界值（两个流程是同一个随机化）；
C 把 BW 组均值极差从 1.215 g 压到 0.225 g 且主指标 100% 达标；
而靠人工换随机数要平均重抽 63 次才能追上 C 第一次抽样的均衡水平。

### 现有实现是不是随机的（`is_it_random.py`）

```
100000 random allocations, 60 -> 3 x 20, min(P) over BW and CD45%
  median 0.2923   q99 0.9013   q99.99 0.9899   maximum 0.9993

P(min(P) >= 0.990000) =   9/100000 = 0.009%
P(min(P) >= 0.998319) =   1/100000 = 0.001%   <- 引擎每次都能给出的水平
```

引擎在同一份数据同一配置下稳定输出 `min_p = 0.998319`（Top-10 全在 0.992–0.998）。
若它是随机抽取，出现这个水平的概率是十万分之一。所以它按性别分层是真的，
但“随机”不是——它是择优搜索。详见设计文档 §4.3。

### 达标率（`real_accept.py`）

真实数据（α = 0.05，Strict）：

```
3 groups x 20               strict pass 90.0%   draws for B 1.11   median min(P) 0.302
5 groups x 12               strict pass 88.3%   draws for B 1.13   median min(P) 0.289
3 groups x 18 + reserve 6   strict pass 90.2%   draws for B 1.11   median min(P) 0.288

BW    invalid in 5.3% of random splits      (CV 5.4%)
CD45% invalid in 5.0% of random splits      (CV 49.4%)
```

最后两行是全套论证的支点：两个指标的变异系数差 9 倍，失衡频率却都等于 α。
随机分配下零假设严格成立，失衡概率就是检验的第一类错误率，与指标变异无关。

合成数据（指标个数的影响）：

```
 1 指标 95.0%    2 指标 84.8%    5 指标 77.5%    10 指标 57.8%
20 指标 29.8%   40 指标  8.0%   70 指标  1.3%
```

## Rust 侧的印证

文档 §4.3 的引擎输出、附录 A.4、A.5 与 §5.4 的优化输出来自一次性探针（临时测试文件，未入库），
它绕过表头识别手工构造 `Dataset`，然后：

- 调 `compute_optimal_grouping` 看优化模式的实际产出与 `total_valid` 占比；
- 同配置连跑两次，验证当前采样路径不可复现；
- 用 `evaluator::score_candidate` 搭一个种子化拒绝采样原型，量化抽样次数。

这些结论在第十章改动清单里都有对应的回归测试项（§11.1、§11.4），
实施时应转成正式测试，而不是再靠临时探针。
