import { useState, useCallback, useEffect } from "react";
import { useAtom } from "jotai";
import {
  datasetAtom,
  groupConfigAtom,
  statConfigAtom,
  selectedIndicatorsAtom,
  currentStepAtom,
} from "@/stores";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { ArrowLeft, ArrowRight, Settings } from "lucide-react";
import type { GroupConfig, StatConfig, SexConstraint } from "@/types";

export function ConfigurePage() {
  const [dataset] = useAtom(datasetAtom);
  const [, setGroupConfig] = useAtom(groupConfigAtom);
  const [, setStatConfig] = useAtom(statConfigAtom);
  const [selectedIndicators, setSelectedIndicators] = useAtom(selectedIndicatorsAtom);
  const [, setCurrentStep] = useAtom(currentStepAtom);

  // Form state
  const [numGroups, setNumGroups] = useState(2);
  const [animalsPerGroup, setAnimalsPerGroup] = useState(5);
  const [alpha, setAlpha] = useState(0.05);
  const [mode, setMode] = useState<"Strict" | "Optimized">("Strict");

  // Dynamic sex constraints array
  const [sexConstraints, setSexConstraints] = useState<SexConstraint[]>([]);

  // Initialize sex constraints when numGroups or dataset changes
  useEffect(() => {
    if (!dataset) return;

    // Calculate average animals per group
    const avgMalesPerGroup = Math.floor(dataset.metadata.male_count / numGroups);
    const avgFemalesPerGroup = Math.floor(dataset.metadata.female_count / numGroups);

    // Initialize constraints with even distribution
    // First group takes the remainder to ensure total matches
    const initialConstraints: SexConstraint[] = Array.from({ length: numGroups }, (_, i) => ({
      group_index: i,
      male_count: i === 0
        ? dataset.metadata.male_count - avgMalesPerGroup * (numGroups - 1)
        : avgMalesPerGroup,
      female_count: i === 0
        ? dataset.metadata.female_count - avgFemalesPerGroup * (numGroups - 1)
        : avgFemalesPerGroup,
    }));

    setSexConstraints(initialConstraints);
  }, [numGroups, dataset]);

  // Update individual sex constraint
  const updateSexConstraint = (groupIndex: number, field: 'male_count' | 'female_count', value: number) => {
    setSexConstraints(prev =>
      prev.map((constraint, i) =>
        i === groupIndex ? { ...constraint, [field]: value } : constraint
      )
    );
  };

  const handleBack = () => {
    setCurrentStep("upload");
  };

  const handleNext = useCallback(() => {
    if (!dataset) return;

    // Build group config using dynamic sex constraints
    const groupConfig: GroupConfig = {
      num_groups: numGroups,
      animals_per_group: {
        type: "Uniform",
        value: animalsPerGroup,
      },
      sex_constraints: sexConstraints,
    };

    // Build stat config
    const statConfig: StatConfig = {
      selected_indicators: selectedIndicators.length > 0
        ? selectedIndicators
        : dataset.indicator_names,
      alpha,
      mode,
    };

    setGroupConfig(groupConfig);
    setStatConfig(statConfig);
    setCurrentStep("compute");
  }, [
    dataset,
    numGroups,
    animalsPerGroup,
    sexConstraints,
    alpha,
    mode,
    selectedIndicators,
    setGroupConfig,
    setStatConfig,
    setCurrentStep,
  ]);

  const toggleIndicator = (indicator: string) => {
    setSelectedIndicators((prev) =>
      prev.includes(indicator)
        ? prev.filter((i) => i !== indicator)
        : [...prev, indicator]
    );
  };

  const selectAllIndicators = () => {
    setSelectedIndicators(dataset?.indicator_names || []);
  };

  const clearAllIndicators = () => {
    setSelectedIndicators([]);
  };

  if (!dataset) {
    return (
      <Alert>
        <AlertDescription>请先上传数据文件</AlertDescription>
      </Alert>
    );
  }

  // Validate sex constraints
  const totalRequired = sexConstraints.reduce((sum, c) => sum + c.male_count + c.female_count, 0);
  const totalMales = sexConstraints.reduce((sum, c) => sum + c.male_count, 0);
  const totalFemales = sexConstraints.reduce((sum, c) => sum + c.female_count, 0);

  const isValid =
    totalRequired === dataset.metadata.total_animals &&
    totalMales === dataset.metadata.male_count &&
    totalFemales === dataset.metadata.female_count;

  return (
    <div className="container max-w-6xl mx-auto py-8 space-y-6">
      {/* Group Configuration */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Settings className="h-5 w-5" />
            分组配置
          </CardTitle>
          <CardDescription>
            配置分组数量和性别约束
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="grid grid-cols-2 gap-6">
            <div className="space-y-2">
              <Label>分组数量</Label>
              <Input
                type="number"
                value={numGroups}
                onChange={(e) => setNumGroups(Number(e.target.value))}
                min={2}
                max={5}
              />
            </div>
            <div className="space-y-2">
              <Label>每组动物数</Label>
              <Input
                type="number"
                value={animalsPerGroup}
                onChange={(e) => setAnimalsPerGroup(Number(e.target.value))}
                min={2}
              />
            </div>
          </div>

          {/* Sex Constraints */}
          <div>
            <Label className="text-base mb-3 block">性别约束</Label>
            <div className="grid grid-cols-2 gap-4">
              {sexConstraints.map((constraint, index) => (
                <Card key={constraint.group_index}>
                  <CardHeader className="pb-3">
                    <CardTitle className="text-sm">组 {index + 1}</CardTitle>
                  </CardHeader>
                  <CardContent className="space-y-3">
                    <div className="space-y-2">
                      <Label className="text-xs">雄性数量</Label>
                      <Input
                        type="number"
                        value={constraint.male_count}
                        onChange={(e) => updateSexConstraint(index, 'male_count', Number(e.target.value))}
                        min={0}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label className="text-xs">雌性数量</Label>
                      <Input
                        type="number"
                        value={constraint.female_count}
                        onChange={(e) => updateSexConstraint(index, 'female_count', Number(e.target.value))}
                        min={0}
                      />
                    </div>
                    <div className="text-xs text-muted-foreground">
                      小计: {constraint.male_count + constraint.female_count} 只
                    </div>
                  </CardContent>
                </Card>
              ))}
            </div>

            {!isValid && (
              <Alert variant="destructive" className="mt-4">
                <AlertDescription>
                  约束配置不正确：需要 {dataset.metadata.total_animals} 只动物
                  ({dataset.metadata.male_count}雄 + {dataset.metadata.female_count}雌)，
                  当前配置 {totalRequired} 只
                  ({totalMales}雄 + {totalFemales}雌)
                </AlertDescription>
              </Alert>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Statistical Configuration */}
      <Card>
        <CardHeader>
          <CardTitle>统计参数</CardTitle>
          <CardDescription>
            配置显著性水平和优化模式
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-2 gap-6">
            <div className="space-y-2">
              <Label>显著性水平 (α)</Label>
              <Input
                type="number"
                value={alpha}
                onChange={(e) => setAlpha(Number(e.target.value))}
                step={0.01}
                min={0.01}
                max={0.1}
              />
              <p className="text-xs text-muted-foreground">
                常用值: 0.05 或 0.01
              </p>
            </div>
            <div className="space-y-2">
              <Label>优化模式</Label>
              <div className="space-y-2 pt-2">
                <div className="flex items-center space-x-2">
                  <input
                    type="radio"
                    id="strict"
                    checked={mode === "Strict"}
                    onChange={() => setMode("Strict")}
                    className="h-4 w-4"
                  />
                  <Label htmlFor="strict" className="font-normal cursor-pointer">
                    严格模式 (所有 P &gt; α)
                  </Label>
                </div>
                <div className="flex items-center space-x-2">
                  <input
                    type="radio"
                    id="optimized"
                    checked={mode === "Optimized"}
                    onChange={() => setMode("Optimized")}
                    className="h-4 w-4"
                  />
                  <Label htmlFor="optimized" className="font-normal cursor-pointer">
                    优化模式 (允许1个 P ≤ α)
                  </Label>
                </div>
              </div>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Indicator Selection */}
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <div>
              <CardTitle>选择参与统计的指标</CardTitle>
              <CardDescription>
                已选择 {selectedIndicators.length > 0 ? selectedIndicators.length : dataset.indicator_names.length} / {dataset.indicator_names.length} 个指标
              </CardDescription>
            </div>
            <div className="flex gap-2">
              <Button variant="outline" size="sm" onClick={selectAllIndicators}>
                全选
              </Button>
              <Button variant="outline" size="sm" onClick={clearAllIndicators}>
                清空
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-3 gap-4 max-h-96 overflow-y-auto">
            {dataset.indicator_names.map((indicator) => (
              <div key={indicator} className="flex items-center space-x-2">
                <Checkbox
                  id={indicator}
                  checked={selectedIndicators.length === 0 || selectedIndicators.includes(indicator)}
                  onCheckedChange={() => toggleIndicator(indicator)}
                />
                <Label
                  htmlFor={indicator}
                  className="text-sm font-normal cursor-pointer"
                >
                  {indicator}
                </Label>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      {/* Navigation */}
      <div className="flex justify-between">
        <Button variant="outline" onClick={handleBack}>
          <ArrowLeft className="mr-2 h-4 w-4" />
          返回
        </Button>
        <Button onClick={handleNext} disabled={!isValid}>
          开始计算
          <ArrowRight className="ml-2 h-4 w-4" />
        </Button>
      </div>
    </div>
  );
}
