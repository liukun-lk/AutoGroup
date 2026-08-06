# 端到端测试固定数据（golden fixtures）

`e2e_grouping_test.rs` 用这两个文件把整条流水线钉死：

```
e2e_input.xlsx  ──解析──▶ Dataset ──分组──▶ GroupingResult ──导出──▶ 与 e2e_expected_output.xlsx 逐格比对
```

| 文件 | 说明 |
|---|---|
| `e2e_input.xlsx` | 真实实验数据：9 只动物（6 雄 + 3 雌），双行表头，解析出 73 个指标键 |
| `e2e_expected_output.xlsx` | 上面这份输入跑出来、并且**经人工确认接受**的导出结果：3 组 × 3 只（2 雄 + 1 雌），70 个指标全部达标 |

## 做过的匿名化

除以下两项外，两个文件与真实产物完全一致——每个测量值、表头、sheet、单元格样式都没动：

1. 动物编号里的研究编号被替换：`XHP26010NN` → `DEMO0NN`（在 `sharedStrings.xml` 里整串替换，因此 `动物编号` / `样本号` / `样品识别号` / `FULLNAME` 四列同步生效）
2. `docProps` 里的 `dc:creator`、`cp:lastModifiedBy` 等文档属性被清空

指标名称（`kg`、`ALT`、`WBC(10^9/L)` 等）是通用实验室缩写，未做处理；数值必须保留原样，否则统计检验就失去了真实性。

## 测试覆盖到什么

- **解析**：双行表头规则、指标键、性别识别、动物数量
- **指标筛选**：73 个键中排除 3 个文本列（`样本号`、`样品识别号`、`FULLNAME`）后应为 70 个，与前端 `src/utils/indicator-filter.ts` 的默认行为一致
- **分组**：候选枚举、检验级联、排序取优，最终选中的动物分配必须与人工确认的那一版逐只相同
- **导出**：三个 sheet（`分组结果` / `统计结果` / `汇总信息`）的全部单元格。文本严格相等；数值按相对误差 `1e-9` 比对，容忍不同平台 libm 的末位差异。`计算耗时 (ms)` 这一行会跳过。

## 这个测试失败了怎么办

**默认假设是代码回归了，不是固定数据过期了。** 失败信息会直接指出 sheet、行、列和两边的值。

只有在你**有意**改变了输出（例如新增一个 sheet、调整导出列）时才更新固定数据，并且要在提交说明里写清改了什么、为什么。重新生成的方式：跑一遍应用导出新结果，再用同样的规则做匿名化：

```python
# 把 sharedStrings 里的研究编号换掉，并清空 docProps
ID_MAP = {f"XHP26010{i:02d}": f"DEMO0{i:02d}" for i in range(1, 10)}
```

如果只是想确认「这份分组在统计上到底对不对」，别改固定数据，先用精确参考实现复核：

```bash
python3 .claude/skills/animal-grouping/scripts/grouping_engine.py verify \
  --excel src-tauri/tests/fixtures/e2e_input.xlsx \
  --assignments <结果 JSON> --alpha 0.05 --mode strict --compare
```
