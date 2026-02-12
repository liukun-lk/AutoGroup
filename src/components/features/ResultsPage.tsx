import { useCallback } from "react";
import { useAtom } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { resultAtom, currentStepAtom, setErrorAtom, datasetAtom, selectedIndicatorsAtom } from "@/stores";
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

export function ResultsPage() {
  const [result] = useAtom(resultAtom);
  const [dataset] = useAtom(datasetAtom);
  const [selectedIndicators] = useAtom(selectedIndicatorsAtom);
  const [, setCurrentStep] = useAtom(currentStepAtom);
  const [, setError] = useAtom(setErrorAtom);

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

      await invoke("export_result", {
        result,
        dataset,
        selectedIndicators: indicatorsToExport,
        outputPath: filePath,
      });

      // Show success message
      alert("导出成功！");
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    }
  }, [result, dataset, selectedIndicators, setError]);

  const handleRestart = () => {
    setCurrentStep("upload");
  };

  if (!result) {
    return (
      <Alert>
        <AlertDescription>未找到计算结果</AlertDescription>
      </Alert>
    );
  }

  const { assignments, statistics, summary, computation_time_ms } = result;

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

  const groups = Object.entries(groupedAssignments)
    .map(([groupId, animals]) => ({
      group_index: Number(groupId),
      animal_ids: animals.map((a) => a.id),
    }))
    .sort((a, b) => a.group_index - b.group_index);

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
              <Card key={group.group_index}>
                <CardHeader className="pb-3">
                  <CardTitle className="text-base">
                    第 {group.group_index + 1} 组
                  </CardTitle>
                  <CardDescription>
                    {group.animal_ids.length} 只动物
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
                  <TableHead className="text-center">状态</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {statistics.map((stat) => {
                  const levenePassed = stat.levene_p_value > 0.05;
                  const diffPassed = stat.diff_p_value > 0.05;
                  const overallPassed = levenePassed && diffPassed;

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
                        {overallPassed ? (
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
              • <strong>Levene检验</strong>: P &gt; 0.05 表示方差齐性良好
            </p>
            <p>
              • <strong>差异检验</strong>: P &gt; 0.05 表示组间无显著差异
            </p>
            <p>
              • <strong>通过</strong>: 两项检验均 P &gt; 0.05
            </p>
          </div>
        </CardContent>
      </Card>

      {/* Action Buttons */}
      <div className="flex justify-between">
        <Button variant="outline" onClick={handleRestart}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          重新开始
        </Button>
        <Button onClick={handleExport} size="lg">
          <Download className="mr-2 h-4 w-4" />
          导出结果
        </Button>
      </div>
    </div>
  );
}
