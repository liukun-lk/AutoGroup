import { useCallback, useState } from "react";
import { useAtom } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import {
  resultAtom,
  setErrorAtom,
  datasetAtom,
  selectedIndicatorsAtom,
  resetStateAtom,
  groupConfigAtom,
  statConfigAtom,
  currentStepAtom,
  groupingRunAtom,
} from "@/stores";
import type { MultiGroupingResult } from "@/types";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Alert, AlertDescription } from "@/components/ui/alert";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  ArrowLeft,
  Download,
  CheckCircle2,
  TrendingUp,
  Users,
  BarChart3,
} from "lucide-react";
import { METHODS, SCENARIOS } from "@/lib/grouping-method";
import { PosthocComparisons } from "./PosthocComparisons";

export function ResultsPage() {
  const [run, setRun] = useAtom(groupingRunAtom);
  const [result] = useAtom(resultAtom);
  const [dataset] = useAtom(datasetAtom);
  const [groupConfig] = useAtom(groupConfigAtom);
  const [statConfig] = useAtom(statConfigAtom);
  const [selectedIndicators] = useAtom(selectedIndicatorsAtom);
  const [, setError] = useAtom(setErrorAtom);
  const [, resetState] = useAtom(resetStateAtom);
  const [, setCurrentStep] = useAtom(currentStepAtom);
  const [redrawing, setRedrawing] = useState(false);

  const isGlp = groupConfig?.scenario === "GlpSubmission";

  const handleExport = useCallback(async () => {
    if (!result || !dataset) return;

    try {
      const filePath = await save({
        filters: [
          {
            name: "Excel Files",
            extensions: ["xlsx"],
          },
        ],
        defaultPath: "grouping_result.xlsx",
      });

      if (!filePath) return;

      // Use selectedIndicators if any were selected, otherwise use all indicators
      const indicatorsToExport = selectedIndicators.length > 0
        ? selectedIndicators
        : dataset.indicator_names;

      // The backend needs the same constraints the grouping was computed with. Without
      // them a reserve group is indistinguishable from an experimental one: it would be
      // labelled 组N instead of its custom name, sorted among the experimental groups,
      // given a mean±SD row it should not have, and counted in 分组数量.
      await invoke("export_result", {
        result,
        dataset,
        selectedIndicators: indicatorsToExport,
        outputPath: filePath,
        groupConstraints: groupConfig?.sex_constraints,
        // The declared scenario is part of "why this grouping was done this way" and is
        // written to the summary sheet next to the principle that was actually used.
        scenario: groupConfig?.scenario,
      });

      // Show success message
      alert("导出成功！");
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }, [result, dataset, selectedIndicators, groupConfig, setError]);

  const handleRestart = () => {
    resetState();
  };

  if (!result) {
    return (
      <Alert>
        <AlertDescription>未找到计算结果</AlertDescription>
      </Alert>
    );
  }

  const { assignments, statistics, summary, computation_time_ms } = result;

  // Validate that we have the necessary data
  if (!assignments || !statistics || !summary) {
    return (
      <Alert>
        <AlertDescription>计算结果数据不完整，请重新计算</AlertDescription>
      </Alert>
    );
  }

  const isRandomizedRun = result.method !== "Optimized";

  const handleSelectCandidate = (index: number) => {
    if (!run || isGlp) return;
    // Functional updater: a redraw in flight resolves against whatever `run` is current
    // at that time, not a stale click-time snapshot from this closure.
    setRun((prev) => (prev ? { ...prev, selectedIndex: index } : prev));
  };

  const handleRedraw = async () => {
    if (!run || !dataset || !statConfig || !groupConfig?.randomization || isGlp) return;
    const lastRecord = run.candidates[run.candidates.length - 1]?.randomization;
    if (!lastRecord) return;
    const nextIndex =
      Math.max(...run.candidates.map((c) => c.randomization?.draw_index ?? 1)) + 1;

    setRedrawing(true);
    try {
      const multi = await invoke<MultiGroupingResult>("compute_grouping", {
        dataset,
        groupConfig: {
          ...groupConfig,
          randomization: {
            ...groupConfig.randomization,
            // The base seed pins the whole draw sequence; the index picks the draw.
            seed: lastRecord.base_seed,
            draw_index: nextIndex,
          },
        },
        statConfig,
      });
      const drawn = multi.candidates[0];
      if (drawn) {
        // Functional updater: a candidate selection made while this await was in flight
        // must not be silently reverted by an update built from the click-time `run`.
        setRun((prev) =>
          prev
            ? {
                ...prev,
                candidates: [...prev.candidates, drawn],
                selectedIndex: prev.candidates.length,
              }
            : prev
        );
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setRedrawing(false);
    }
  };

  // Transform flat assignments into grouped structure
  const groupedAssignments = assignments.reduce<Record<number, Array<{ id: string; sex: string }>>>(
    (acc, assignment) => {
      if (!acc[assignment.group_id]) {
        acc[assignment.group_id] = [];
      }
      acc[assignment.group_id].push({
        id: assignment.animal_id,
        sex: assignment.sex,
      });
      return acc;
    },
    {}
  );

  // Reserve animals share the assignment list with the experimental groups, so label
  // them from the configuration instead of numbering them as another group.
  const reserveGroupIds = new Set(
    (groupConfig?.sex_constraints ?? [])
      .filter((c) => c.group_type === "Reserve")
      .map((c) => c.group_index)
  );

  const groups = Object.entries(groupedAssignments)
    .map(([groupId, animals]) => {
      const group_index = Number(groupId);
      const isReserve = reserveGroupIds.has(group_index);
      return {
        group_index,
        isReserve,
        label: isReserve ? "备用动物" : `第 ${group_index + 1} 组`,
        animal_ids: animals.map((a) => a.id),
      };
    })
    .sort((a, b) => a.group_index - b.group_index);

  const record = result.randomization ?? null;
  const methodLabel = METHODS.find((m) => m.value === result.method)?.label ?? "统计均衡优化";
  const scenarioLabel =
    SCENARIOS.find((s) => s.value === groupConfig?.scenario)?.label ?? "探索性 / 非 GLP 实验";

  const alpha = statConfig?.alpha ?? 0.05;
  const groupLabels = new Map(groups.map((g) => [g.group_index, g.label]));
  const labelFor = (groupId: number) => groupLabels.get(groupId) ?? `第 ${groupId + 1} 组`;

  return (
    <div className="container max-w-7xl mx-auto py-8 space-y-6">
      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">总动物数</CardTitle>
            <Users className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{summary.total_animals}</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">分组数量</CardTitle>
            <BarChart3 className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{summary.num_groups}</div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">合格指标</CardTitle>
            <CheckCircle2 className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold text-green-600">
              {summary.passed_indicators}
            </div>
            <p className="text-xs text-muted-foreground">
              / {summary.total_indicators}
            </p>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
            <CardTitle className="text-sm font-medium">计算耗时</CardTitle>
            <TrendingUp className="h-4 w-4 text-muted-foreground" />
          </CardHeader>
          <CardContent>
            <div className="text-2xl font-bold">{computation_time_ms.toFixed(0)}</div>
            <p className="text-xs text-muted-foreground">ms</p>
          </CardContent>
        </Card>
      </div>

      {/* Candidate switcher */}
      {run && (isRandomizedRun || run.candidates.length > 1) && (
        <Card>
          <CardHeader>
            <CardTitle>候选分组</CardTitle>
            <CardDescription>
              {isRandomizedRun
                ? "每一签都由（主种子，抽签序号）唯一决定，可随时复现；抽过的签全部保留"
                : "优化模式返回的 Top-N 排名，按 min(P) 与 mean(P) 降序"}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap gap-2">
              {run.candidates.map((candidate, index) => (
                <Button
                  key={index}
                  size="sm"
                  variant={index === run.selectedIndex ? "default" : "outline"}
                  disabled={isGlp}
                  onClick={() => handleSelectCandidate(index)}
                >
                  {isRandomizedRun
                    ? `第 ${candidate.randomization?.draw_index ?? index + 1} 签`
                    : `排名 #${index + 1} · min(P)=${candidate.summary.min_p_value.toFixed(4)}`}
                </Button>
              ))}
              {isRandomizedRun && (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={isGlp || redrawing}
                  onClick={handleRedraw}
                >
                  {redrawing ? "抽签中…" : "再抽一签"}
                </Button>
              )}
            </div>
            {isGlp && (
              <Alert>
                <AlertDescription className="text-sm">
                  {"GLP 场景执行分配隐藏：一次抽签即为最终分配，不提供看到结果后重抽或挑选的入口。需要更高的均衡度，请在计算前调整接受准则的目标接受率。"}
                </AlertDescription>
              </Alert>
            )}
            <p className="text-xs text-muted-foreground">
              导出将使用当前选中的候选（
              {isRandomizedRun
                ? `第 ${result.randomization?.draw_index ?? run.selectedIndex + 1} 签`
                : `排名 #${run.selectedIndex + 1}`}
              ）。
            </p>
          </CardContent>
        </Card>
      )}

      {/* Method and audit trail */}
      <Card>
        <CardHeader>
          <CardTitle>分组方法与可追溯性</CardTitle>
          <CardDescription>导出文件的「汇总信息」会记录同样的内容，供审计与复现使用</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 md:grid-cols-3 gap-3 text-sm">
            <div>
              <div className="text-muted-foreground text-xs">应用场景</div>
              <div className="font-medium">{scenarioLabel}</div>
            </div>
            <div>
              <div className="text-muted-foreground text-xs">分组方式</div>
              <div className="font-medium">{methodLabel}</div>
            </div>
            {record?.primary_indicator && (
              <div>
                <div className="text-muted-foreground text-xs">分层变量</div>
                <div className="font-medium">
                  {record.primary_indicator}
                  {record.block_size ? `（区组大小 ${record.block_size}）` : ""}
                </div>
              </div>
            )}
            {record && (
              <>
                <div>
                  <div className="text-muted-foreground text-xs">随机种子</div>
                  <div className="font-medium font-mono">{record.seed}</div>
                </div>
                <div>
                  <div className="text-muted-foreground text-xs">随机数算法</div>
                  <div className="font-medium font-mono">{record.rng_algorithm}</div>
                </div>
                <div>
                  <div className="text-muted-foreground text-xs">抽样次数</div>
                  <div className="font-medium">{record.attempts}</div>
                </div>
                <div>
                  <div className="text-muted-foreground text-xs">输入指纹</div>
                  <div className="font-medium font-mono">{record.input_fingerprint}</div>
                </div>
                <div>
                  <div className="text-muted-foreground text-xs">引擎版本</div>
                  <div className="font-medium font-mono">{record.engine_version}</div>
                </div>
                <div>
                  <div className="text-muted-foreground text-xs">抽签序号</div>
                  <div className="font-medium">第 {record.draw_index} 签</div>
                </div>
                {record.draw_index > 1 && (
                  <div>
                    <div className="text-muted-foreground text-xs">主种子</div>
                    <div className="font-medium font-mono">{record.base_seed}</div>
                  </div>
                )}
                <div>
                  <div className="text-muted-foreground text-xs">接受准则</div>
                  <div className="font-medium">
                    {record.acceptance == null
                      ? "无（纯随机）"
                      : record.acceptance.type === "AlphaLine"
                        ? "全部所选指标 P > α"
                        : `仅接受最均衡的前 ${Math.round(record.acceptance.target_rate * 100)}%（min(P) ≥ ${record.calibrated_threshold?.toFixed(4) ?? "—"}）`}
                  </div>
                </div>
              </>
            )}
          </div>

          {record && (
            <div className="rounded-md bg-muted/50 p-3 text-sm space-y-1">
              <div className="font-medium">复现步骤</div>
              <ol className="list-decimal list-inside text-muted-foreground space-y-0.5">
                <li>导入同一份数据文件（软件校验输入指纹一致）；</li>
                <li>选择相同的场景、方法与参数，在种子栏填入上方记录的随机种子；</li>
                <li>重新计算，得到的分配与本次逐动物一致。</li>
              </ol>
              <p className="text-xs text-muted-foreground">
                以上信息已随导出文件写入《汇总信息》表，归档请以导出文件为准。
              </p>
            </div>
          )}

          {record?.primary_indicator && (
            <Alert>
              <AlertDescription className="text-sm">
                <strong>{record.primary_indicator}</strong> 是本次分组的
                <strong>分层变量</strong>，不是检验指标。分层设计下它的组间检验 P 值必然接近
                1，这是构造的结果，不能当作「均衡性极佳」的证据。报告中应同时给出各组该指标的均值 ±
                标准差，并注明它是分层变量。
              </AlertDescription>
            </Alert>
          )}

          {!summary.meets_criteria && (
            <Alert variant="destructive">
              <AlertDescription className="text-sm">
                本次分组有 {summary.num_invalid_indicators} 个指标未达到均衡要求。
                {record
                  ? "请勿反复更换种子重算——那是看到结果之后的挑选。可行的做法是开启接受准则，或改用按主指标分层随机，两者都是事先声明的规则。"
                  : "可调整参与统计的指标或放宽判定口径后重算。"}
              </AlertDescription>
            </Alert>
          )}

          {record && summary.total_indicators !== selectedIndicators.length && (
            <Alert>
              <AlertDescription className="text-sm">
                所选 {selectedIndicators.length} 个指标中，实际参与检验的为{" "}
                {summary.total_indicators} 个：其余指标在本次划分下无法计算检验（组内取值不足或方差为零），已跳过。
              </AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>

      {/* Group Assignments */}
      <Card>
        <CardHeader>
          <CardTitle>分组结果</CardTitle>
          <CardDescription>
            每组动物的分配情况
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {groups.map((group) => (
              <Card
                key={group.group_index}
                className={group.isReserve ? "border-dashed border-2 bg-muted/20" : undefined}
              >
                <CardHeader className="pb-3">
                  <CardTitle className="text-base">{group.label}</CardTitle>
                  <CardDescription>
                    {group.animal_ids.length} 只动物
                    {group.isReserve && "（不参与统计）"}
                  </CardDescription>
                </CardHeader>
                <CardContent>
                  <div className="flex flex-wrap gap-1">
                    {group.animal_ids.map((id) => (
                      <span
                        key={id}
                        className="inline-flex items-center px-2 py-1 rounded-md bg-primary/10 text-xs font-medium"
                      >
                        {id}
                      </span>
                    ))}
                  </div>
                </CardContent>
              </Card>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Statistical Results */}
      <Card>
        <CardHeader>
          <CardTitle>统计检验结果</CardTitle>
          <CardDescription>
            各指标的方差齐性检验(Levene)和组间差异检验(t/ANOVA) P值
          </CardDescription>
        </CardHeader>
        <CardContent>
          <div className="rounded-md border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>指标名称</TableHead>
                  <TableHead className="text-center">Levene P值</TableHead>
                  <TableHead className="text-center">差异检验 P值</TableHead>
                  <TableHead className="text-center">最严格两两比较</TableHead>
                  <TableHead className="text-center">状态</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {statistics.map((stat) => {
                  // Levene only selects which test to run; it is not a pass/fail criterion.
                  // The verdict comes from the backend, which applies the full rule:
                  // main test P > alpha AND every pairwise comparison P > alpha.
                  const levenePassed = stat.levene_p_value > alpha;
                  const diffPassed = stat.diff_p_value > alpha;
                  const comparisons = stat.posthoc_results ?? [];
                  const worstPosthoc = comparisons.length
                    ? comparisons.reduce((a, b) => (b.p_value < a.p_value ? b : a))
                    : null;

                  return (
                    <TableRow key={stat.indicator_name}>
                      <TableCell className="font-medium">
                        {stat.indicator_name}
                      </TableCell>
                      <TableCell className="text-center">
                        <span
                          className={
                            levenePassed ? "text-green-600" : "text-amber-600"
                          }
                        >
                          {stat.levene_p_value.toFixed(4)}
                        </span>
                      </TableCell>
                      <TableCell className="text-center">
                        <span
                          className={
                            diffPassed ? "text-green-600" : "text-amber-600"
                          }
                        >
                          {stat.diff_p_value.toFixed(4)}
                        </span>
                      </TableCell>
                      <TableCell className="text-center">
                        {worstPosthoc ? (
                          <span
                            className={
                              worstPosthoc.is_valid
                                ? "text-green-600"
                                : "text-amber-600"
                            }
                          >
                            {worstPosthoc.p_value.toFixed(4)}
                            <span className="ml-1 text-xs text-muted-foreground">
                              {labelFor(worstPosthoc.group1_id)}/
                              {labelFor(worstPosthoc.group2_id)}
                            </span>
                          </span>
                        ) : (
                          <span className="text-muted-foreground">—</span>
                        )}
                      </TableCell>
                      <TableCell className="text-center">
                        {stat.is_valid ? (
                          <span className="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-green-100 text-green-700">
                            通过
                          </span>
                        ) : (
                          <span className="inline-flex items-center px-2 py-1 rounded-full text-xs font-medium bg-amber-100 text-amber-700">
                            警告
                          </span>
                        )}
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          </div>

          {/* Legend */}
          <div className="mt-4 text-sm text-muted-foreground">
            <p>
              • <strong>Levene检验</strong>: P &gt; {alpha} 表示方差齐性良好，决定采用哪种差异检验
            </p>
            <p>
              • <strong>差异检验</strong>: P &gt; {alpha} 表示组间整体无显著差异
            </p>
            <p>
              • <strong>最严格两两比较</strong>: 该指标所有组间两两比较中 P 值最小的一对
            </p>
            <p>
              • <strong>通过</strong>: 差异检验与全部两两比较的 P 均 &gt; {alpha}
            </p>
          </div>
        </CardContent>
      </Card>

      {/* Pairwise post-hoc comparisons */}
      <PosthocComparisons statistics={statistics} alpha={alpha} labelOf={labelFor} />

      {/* Action Buttons */}
      <div className="flex justify-between">
        <Button variant="outline" onClick={handleRestart}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          重新开始
        </Button>
        <Button variant="outline" onClick={() => setCurrentStep("configure")}>
          返回修改配置
        </Button>
        <Button onClick={handleExport} size="lg">
          <Download className="mr-2 h-4 w-4" />
          导出结果
        </Button>
      </div>
    </div>
  );
}
