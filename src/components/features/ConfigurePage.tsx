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
import { ArrowLeft, ArrowRight, Settings, Info } from "lucide-react";
import type { GroupConfig, StatConfig, SexConstraint } from "@/types";
import { getExcludedIndicators, filterDefaultIndicators } from "@/utils/indicator-filter";

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

  // Reserve group state
  const [reserveMaleCount, setReserveMaleCount] = useState(0);
  const [reserveFemaleCount, setReserveFemaleCount] = useState(0);

  // Dynamic sex constraints array (for experimental groups only)
  const [sexConstraints, setSexConstraints] = useState<SexConstraint[]>([]);

  // Track if default indicators have been initialized
  const [defaultsInitialized, setDefaultsInitialized] = useState(false);

  // Initialize default selected indicators (only once per dataset)
  useEffect(() => {
    if (!dataset || defaultsInitialized) return;

    // Only initialize if selectedIndicators is empty (first load)
    if (selectedIndicators.length === 0) {
      const defaultIndicators = filterDefaultIndicators(dataset.indicator_names);
      setSelectedIndicators(defaultIndicators);
    }

    setDefaultsInitialized(true);
  }, [dataset, selectedIndicators.length, defaultsInitialized, setSelectedIndicators]);

  // Reset initialization flag when dataset changes
  useEffect(() => {
    setDefaultsInitialized(false);
  }, [dataset]);

  // Initialize sex constraints when numGroups or dataset changes
  useEffect(() => {
    if (!dataset) return;

    // Calculate available animals after reserve group allocation
    const availableMales = dataset.metadata.male_count - reserveMaleCount;
    const availableFemales = dataset.metadata.female_count - reserveFemaleCount;

    // Calculate average animals per experimental group
    const avgMalesPerGroup = Math.floor(availableMales / numGroups);
    const avgFemalesPerGroup = Math.floor(availableFemales / numGroups);

    // Initialize constraints with even distribution
    // First group takes the remainder to ensure total matches
    const initialConstraints: SexConstraint[] = Array.from({ length: numGroups }, (_, i) => ({
      group_index: i,
      male_count: i === 0
        ? availableMales - avgMalesPerGroup * (numGroups - 1)
        : avgMalesPerGroup,
      female_count: i === 0
        ? availableFemales - avgFemalesPerGroup * (numGroups - 1)
        : avgFemalesPerGroup,
      group_type: "Experimental",
    }));

    setSexConstraints(initialConstraints);
  }, [numGroups, dataset, reserveMaleCount, reserveFemaleCount]);

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
    if (!dataset || selectedIndicators.length === 0) {
      return;
    }

    // Build complete sex constraints: experimental groups + reserve group
    const allConstraints: SexConstraint[] = [
      ...sexConstraints,
      // Add reserve group as the last constraint
      {
        group_index: numGroups,
        male_count: reserveMaleCount,
        female_count: reserveFemaleCount,
        group_type: "Reserve",
        custom_name: "备用动物",
      },
    ];

    // Build group config using complete constraints
    const groupConfig: GroupConfig = {
      num_groups: numGroups + 1, // Include reserve group
      animals_per_group: {
        type: "Uniform",
        value: animalsPerGroup,
      },
      sex_constraints: allConstraints,
    };

    // Build stat config
    const statConfig: StatConfig = {
      selected_indicators: selectedIndicators,
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
    reserveMaleCount,
    reserveFemaleCount,
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

  // Get list of excluded indicators for display
  const excludedIndicators = dataset ? getExcludedIndicators(dataset.indicator_names) : [];

  if (!dataset) {
    return (
      <Alert>
        <AlertDescription>请先上传数据文件</AlertDescription>
      </Alert>
    );
  }

  // Validate sex constraints
  const totalRequired = sexConstraints.reduce((sum, c) => sum + c.male_count + c.female_count, 0)
    + reserveMaleCount + reserveFemaleCount;
  const totalMales = sexConstraints.reduce((sum, c) => sum + c.male_count, 0) + reserveMaleCount;
  const totalFemales = sexConstraints.reduce((sum, c) => sum + c.female_count, 0) + reserveFemaleCount;

  const areSexConstraintsValid =
    totalRequired === dataset.metadata.total_animals &&
    totalMales === dataset.metadata.male_count &&
    totalFemales === dataset.metadata.female_count;

  const hasSelectedIndicators = selectedIndicators.length > 0;

  const isValid = areSexConstraintsValid && hasSelectedIndicators;

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
              {/* Reserve Group Card - Always first with special styling */}
              <Card className="border-dashed border-2 border-muted-foreground/30 bg-muted/20">
                <CardHeader className="pb-3">
                  <CardTitle className="text-sm flex items-center justify-between">
                    <span>备用动物</span>
                    <span className="text-xs font-normal text-muted-foreground">
                      (不参与统计)
                    </span>
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-3">
                  <div className="space-y-2">
                    <Label className="text-xs">雄性数量</Label>
                    <Input
                      type="number"
                      value={reserveMaleCount}
                      onChange={(e) => setReserveMaleCount(Number(e.target.value))}
                      min={0}
                      max={dataset.metadata.male_count}
                    />
                  </div>
                  <div className="space-y-2">
                    <Label className="text-xs">雌性数量</Label>
                    <Input
                      type="number"
                      value={reserveFemaleCount}
                      onChange={(e) => setReserveFemaleCount(Number(e.target.value))}
                      min={0}
                      max={dataset.metadata.female_count}
                    />
                  </div>
                  <div className="text-xs text-muted-foreground">
                    小计: {reserveMaleCount + reserveFemaleCount} 只
                  </div>
                </CardContent>
              </Card>

              {/* Experimental Groups */}
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

            {!areSexConstraintsValid && (
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
                已选择 {selectedIndicators.length} / {dataset.indicator_names.length} 个指标
              </CardDescription>
            </div>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={selectAllIndicators}
                disabled={selectedIndicators.length === dataset.indicator_names.length}
              >
                全选
              </Button>
              <Button
                variant="outline"
                size="sm"
                onClick={clearAllIndicators}
                disabled={selectedIndicators.length === 0}
              >
                清空
              </Button>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {/* Info about excluded indicators */}
          {excludedIndicators.length > 0 && (
            <Alert className="mb-4">
              <Info className="h-4 w-4" />
              <AlertDescription>
                <div className="font-medium mb-1">自动过滤提示</div>
                <div className="text-sm">
                  以下 {excludedIndicators.length} 个字段已默认排除（可手动选择）：
                  <span className="text-muted-foreground ml-1">
                    {excludedIndicators.join(", ")}
                  </span>
                </div>
              </AlertDescription>
            </Alert>
          )}

          <div className="grid grid-cols-3 gap-4 max-h-96 overflow-y-auto">
            {dataset.indicator_names.map((indicator) => (
              <div key={indicator} className="flex items-center space-x-2">
                <Checkbox
                  id={indicator}
                  checked={selectedIndicators.includes(indicator)}
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
        <div className="flex flex-col items-end gap-2">
          {!hasSelectedIndicators && (
            <span className="text-xs text-destructive">
              请至少选择一个参与统计的指标
            </span>
          )}
          <Button onClick={handleNext} disabled={!isValid}>
            开始计算
            <ArrowRight className="ml-2 h-4 w-4" />
          </Button>
        </div>
      </div>
    </div>
  );
}
