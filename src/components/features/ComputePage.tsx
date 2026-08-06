import { useEffect, useState } from "react";
import { useAtom } from "jotai";
import { invoke } from "@tauri-apps/api/core";
import {
  datasetAtom,
  groupConfigAtom,
  statConfigAtom,
  resultAtom,
  currentStepAtom,
  setErrorAtom,
} from "@/stores";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Loader2, CheckCircle2, AlertCircle } from "lucide-react";
import type { MultiGroupingResult } from "@/types";

export function ComputePage() {
  const [dataset] = useAtom(datasetAtom);
  const [groupConfig] = useAtom(groupConfigAtom);
  const [statConfig] = useAtom(statConfigAtom);
  const [, setResult] = useAtom(resultAtom);
  const [, setCurrentStep] = useAtom(currentStepAtom);
  const [, setError] = useAtom(setErrorAtom);

  const [status, setStatus] = useState<"computing" | "success" | "error">("computing");
  const [progress, setProgress] = useState(0);
  const [computationTime, setComputationTime] = useState<number>(0);

  // Reserve animals are not an experimental group, so they must not inflate the
  // group count shown to the user.
  const experimentalGroups =
    groupConfig?.sex_constraints.filter((c) => c.group_type !== "Reserve").length ?? 0;
  const reserveAnimals =
    groupConfig?.sex_constraints
      .filter((c) => c.group_type === "Reserve")
      .reduce((sum, c) => sum + c.male_count + c.female_count, 0) ?? 0;

  useEffect(() => {
    if (!dataset || !groupConfig || !statConfig) {
      setError("Missing required configuration");
      setStatus("error");
      return;
    }

    let progressInterval: number;

    const compute = async () => {
      try {
        setStatus("computing");
        setProgress(0);

        // Simulate progress animation
        progressInterval = window.setInterval(() => {
          setProgress((prev) => {
            if (prev >= 90) return prev;
            return prev + Math.random() * 10;
          });
        }, 200);

        const startTime = performance.now();

        // Call Rust backend - now returns MultiGroupingResult
        const multiResult = await invoke<MultiGroupingResult>("compute_grouping", {
          dataset,
          groupConfig,
          statConfig,
        });

        const endTime = performance.now();
        const elapsed = endTime - startTime;

        clearInterval(progressInterval);
        setProgress(100);
        setComputationTime(elapsed);

        // Select the best candidate from multi-result
        // Backend already sorts by: 1) max min_p_value, 2) max mean_p_value
        // We re-sort here as a safety measure to ensure consistency
        if (multiResult.candidates && multiResult.candidates.length > 0) {
          const sortedCandidates = [...multiResult.candidates].sort((a, b) => {
            // Primary: compare min_p_value (descending - higher is better)
            const minPDiff = b.summary.min_p_value - a.summary.min_p_value;
            if (Math.abs(minPDiff) > 1e-10) {
              return minPDiff;
            }
            // Secondary: compare mean_p_value (descending - higher is better)
            return b.summary.mean_p_value - a.summary.mean_p_value;
          });

          const bestResult = sortedCandidates[0];
          setResult(bestResult);
          setStatus("success");

          // Auto-navigate to results after a brief delay
          setTimeout(() => {
            setCurrentStep("results");
          }, 1500);
        } else {
          throw new Error("No valid grouping solution found");
        }
      } catch (error) {
        clearInterval(progressInterval);
        setStatus("error");
        setError(error instanceof Error ? error.message : String(error));
      }
    };

    compute();

    return () => {
      if (progressInterval) {
        clearInterval(progressInterval);
      }
    };
  }, [dataset, groupConfig, statConfig, setResult, setCurrentStep, setError]);

  return (
    <div className="container max-w-4xl mx-auto py-8">
      <Card>
        <CardHeader>
          <CardTitle className="text-2xl">计算最优分组方案</CardTitle>
          <CardDescription>
            正在使用统计优化算法计算分组结果
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          {/* Status Display */}
          <div className="flex flex-col items-center justify-center py-12">
            {status === "computing" && (
              <>
                <Loader2 className="h-16 w-16 text-primary animate-spin mb-4" />
                <h3 className="text-lg font-semibold mb-2">正在计算...</h3>
                <p className="text-sm text-muted-foreground mb-6">
                  这可能需要几秒到几分钟，取决于数据量和分组配置
                </p>
              </>
            )}

            {status === "success" && (
              <>
                <CheckCircle2 className="h-16 w-16 text-green-600 mb-4" />
                <h3 className="text-lg font-semibold mb-2">计算完成！</h3>
                <p className="text-sm text-muted-foreground mb-2">
                  耗时: {computationTime.toFixed(2)} ms
                </p>
                <p className="text-sm text-muted-foreground">
                  正在跳转到结果页面...
                </p>
              </>
            )}

            {status === "error" && (
              <>
                <AlertCircle className="h-16 w-16 text-destructive mb-4" />
                <h3 className="text-lg font-semibold mb-2">计算失败</h3>
                <p className="text-sm text-muted-foreground">
                  请检查配置后重试
                </p>
              </>
            )}
          </div>

          {/* Progress Bar */}
          {status === "computing" && (
            <div className="space-y-2">
              <Progress value={progress} className="h-2" />
              <p className="text-xs text-center text-muted-foreground">
                {Math.round(progress)}%
              </p>
            </div>
          )}

          {/* Computation Details */}
          {groupConfig && statConfig && (
            <div className="bg-muted/50 rounded-lg p-4 space-y-2">
              <h4 className="font-medium text-sm mb-3">计算参数：</h4>
              <div className="grid grid-cols-2 gap-2 text-sm">
                <div>
                  <span className="text-muted-foreground">分组数量:</span>
                  <span className="ml-2 font-medium">{experimentalGroups}</span>
                  {reserveAnimals > 0 && (
                    <span className="ml-2 text-xs text-muted-foreground">
                      (另有备用动物 {reserveAnimals} 只)
                    </span>
                  )}
                </div>
                <div>
                  <span className="text-muted-foreground">每组动物数:</span>
                  <span className="ml-2 font-medium">
                    {groupConfig.animals_per_group.type === "Uniform"
                      ? groupConfig.animals_per_group.value
                      : "自定义"}
                  </span>
                </div>
                <div>
                  <span className="text-muted-foreground">显著性水平:</span>
                  <span className="ml-2 font-medium">{statConfig.alpha}</span>
                </div>
                <div>
                  <span className="text-muted-foreground">优化模式:</span>
                  <span className="ml-2 font-medium">
                    {statConfig.mode === "Strict" ? "严格模式" : "优化模式"}
                  </span>
                </div>
                <div className="col-span-2">
                  <span className="text-muted-foreground">参与指标:</span>
                  <span className="ml-2 font-medium">
                    {statConfig.selected_indicators.length > 0
                      ? statConfig.selected_indicators.length
                      : dataset?.indicator_names.length || 0}{" "}
                    个
                  </span>
                </div>
              </div>
            </div>
          )}

          {/* Error Alert */}
          {status === "error" && (
            <Alert variant="destructive">
              <AlertDescription>
                计算过程出现错误，请检查配置是否正确或联系技术支持
              </AlertDescription>
            </Alert>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
