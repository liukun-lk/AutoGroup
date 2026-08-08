import { useState, useCallback, useEffect, useRef, useMemo } from "react";
import { useAtom } from "jotai";
import {
  datasetAtom,
  groupConfigAtom,
  statConfigAtom,
  selectedIndicatorsAtom,
  currentStepAtom,
  groupingRunAtom,
} from "@/stores";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Checkbox } from "@/components/ui/checkbox";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { ArrowLeft, ArrowRight, Settings, Info, Dices } from "lucide-react";
import type {
  AcceptanceCriterion,
  GroupConfig,
  GroupingMethod,
  RandomizationConfig,
  SexConstraint,
  StatConfig,
  StudyScenario,
} from "@/types";
import { getExcludedIndicators, filterDefaultIndicators } from "@/utils/indicator-filter";
import {
  ACCEPTANCE_FOOTNOTE,
  ACCEPTANCE_TIERS,
  DEFAULT_ALLOCATION_PROBABILITY,
  METHODS,
  SCENARIOS,
  TARGET_RATE_PRESETS,
  blockStructure,
  defaultMethodFor,
  disabledReason,
  usesRandomSource,
} from "@/lib/grouping-method";

export function ConfigurePage() {
  const [dataset] = useAtom(datasetAtom);
  const [storedGroupConfig, setGroupConfig] = useAtom(groupConfigAtom);
  const [storedStatConfig, setStatConfig] = useAtom(statConfigAtom);
  const [selectedIndicators, setSelectedIndicators] = useAtom(selectedIndicatorsAtom);
  const [, setCurrentStep] = useAtom(currentStepAtom);
  const [existingRun] = useAtom(groupingRunAtom);

  // The previously submitted config, if any — used to hydrate every control below when
  // the user returns from the results page via "返回修改配置".
  const storedExperimental = useMemo(
    () =>
      storedGroupConfig?.sex_constraints.filter((c) => c.group_type !== "Reserve") ?? [],
    [storedGroupConfig]
  );
  const storedReserve = storedGroupConfig?.sex_constraints.find(
    (c) => c.group_type === "Reserve"
  );

  // Form state
  const [numGroups, setNumGroups] = useState(() => storedExperimental.length || 2);
  const [animalsPerGroup, setAnimalsPerGroup] = useState(() =>
    storedGroupConfig?.animals_per_group.type === "Uniform"
      ? storedGroupConfig.animals_per_group.value
      : 5
  );
  const [alpha, setAlpha] = useState(() => storedStatConfig?.alpha ?? 0.05);
  const [mode, setMode] = useState<"Strict" | "Optimized">(
    () => storedStatConfig?.mode ?? "Strict"
  );

  // Scenario first, method second: the scenario decides which methods are on offer.
  const [scenario, setScenario] = useState<StudyScenario>(
    () => storedGroupConfig?.scenario ?? "Exploratory"
  );
  const [method, setMethod] = useState<GroupingMethod>(
    () => storedGroupConfig?.method ?? defaultMethodFor("Exploratory")
  );
  const [methodNotice, setMethodNotice] = useState<string | null>(null);
  const [primaryIndicator, setPrimaryIndicator] = useState<string>(
    () => storedGroupConfig?.randomization?.primary_indicator ?? ""
  );
  const [seedText, setSeedText] = useState<string>(() => {
    const storedSeed = storedGroupConfig?.randomization?.seed;
    if (storedSeed != null) return storedSeed.toString();
    // A blank seed field let the backend generate one; it only ever landed in the run's
    // record, never back in groupConfigAtom. Hydrate base_seed (not the per-draw seed) so
    // an unchanged recompute replays the same sequence instead of drawing a fresh one —
    // `existingRun` (declared above, at the top of the component) must stay in scope here.
    const recorded = existingRun?.candidates[existingRun.selectedIndex]?.randomization?.base_seed;
    return recorded != null ? recorded.toString() : "";
  });
  const [acceptanceTier, setAcceptanceTier] = useState<"alpha" | "topfraction">(() =>
    storedGroupConfig?.randomization?.acceptance?.type === "TopFraction"
      ? "topfraction"
      : "alpha"
  );
  const [targetRate, setTargetRate] = useState(() =>
    storedGroupConfig?.randomization?.acceptance?.type === "TopFraction"
      ? storedGroupConfig.randomization.acceptance.target_rate
      : 0.1
  );
  /** BlockedRandom only: whether the (optional) criterion is on at all. */
  const [criterionOn, setCriterionOn] = useState(() =>
    storedGroupConfig?.method === "BlockedRandom"
      ? storedGroupConfig.randomization?.acceptance != null
      : true
  );

  // Minimization: what to balance on, and how strongly the imbalance measure is obeyed.
  // No default covariate is preselected, for the same reason as the primary indicator —
  // only the experimenter knows which covariates the study hinges on.
  const [covariates, setCovariates] = useState<string[]>(
    () => storedGroupConfig?.randomization?.minimization?.covariates ?? []
  );
  const [allocationProbabilityText, setAllocationProbabilityText] = useState(() =>
    (
      storedGroupConfig?.randomization?.minimization?.allocation_probability ??
      DEFAULT_ALLOCATION_PROBABILITY
    ).toString()
  );

  // Reserve group state
  const [reserveMaleCount, setReserveMaleCount] = useState(() => storedReserve?.male_count ?? 0);
  const [reserveFemaleCount, setReserveFemaleCount] = useState(
    () => storedReserve?.female_count ?? 0
  );

  // Track which field user is actively controlling (for linkage logic)
  const [lastEditedField, setLastEditedField] = useState<"groups" | "animals">("groups");

  // Dynamic sex constraints array (for experimental groups only)
  const [sexConstraints, setSexConstraints] = useState<SexConstraint[]>([]);

  // Track if default indicators have been initialized
  const [defaultsInitialized, setDefaultsInitialized] = useState(false);

  // Calculate available animals for experimental groups
  const availableAnimals = dataset
    ? dataset.metadata.total_animals - reserveMaleCount - reserveFemaleCount
    : 0;

  // Calculate configuration status
  const requiredAnimals = numGroups * animalsPerGroup;
  const surplus = availableAnimals - requiredAnimals;
  const hasConflict = surplus !== 0;
  const canDivideEvenly = availableAnimals % numGroups === 0;

  // Switching scenarios resets the method to that scenario's default and says so;
  // silently swapping it would leave the user believing they were still running the
  // method they picked. Primary indicator, seed and reserve settings are kept.
  const scenarioInitialized = useRef(false);
  useEffect(() => {
    if (!scenarioInitialized.current) {
      scenarioInitialized.current = true;
      return;
    }

    const next = defaultMethodFor(scenario);
    setMethod((current) => {
      if (current === next) return current;
      const name = (m: GroupingMethod) => METHODS.find((entry) => entry.value === m)?.label ?? m;
      setMethodNotice(`分组方式已从「${name(current)}」改为「${name(next)}」`);
      return next;
    });
  }, [scenario]);

  const selectedMethod = METHODS.find((m) => m.value === method);
  // Two different questions. `isRandomized` is the pure randomization family, which is
  // what the acceptance criterion belongs to; `usesSeed` is "does this method draw from
  // the seeded stream", which minimization also does.
  const isRandomized =
    method === "Random" || method === "ConstrainedRandom" || method === "BlockedRandom";
  const isMinimization = method === "Minimization";
  const usesSeed = usesRandomSource(method);

  // Only indicators every animal actually has a number for can define blocks: an animal
  // without a value has no block to sit in. This is the same rule the backend enforces.
  const numericIndicators = useMemo(() => {
    if (!dataset) return [];
    return dataset.indicator_names.filter((name) =>
      dataset.animals.every((animal) => typeof animal.indicators[name] === "number")
    );
  }, [dataset]);

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
  const constraintsHydrated = useRef(false);
  useEffect(() => {
    if (!dataset) return;

    // On the first run after mount, prefer the constraints carried over from a
    // previously submitted config over rebuilding an even split — otherwise returning
    // from the results page via "返回修改配置" would silently discard hand-edited quotas.
    if (!constraintsHydrated.current) {
      constraintsHydrated.current = true;
      if (storedExperimental.length > 0 && storedExperimental.length === numGroups) {
        setSexConstraints(storedExperimental);
        return;
      }
    }

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
  }, [numGroups, dataset, reserveMaleCount, reserveFemaleCount, storedExperimental]);

  // Linkage effect: auto-adjust based on last edited field
  useEffect(() => {
    if (!dataset || availableAnimals <= 0) return;

    if (lastEditedField === "groups") {
      // User edited numGroups -> auto-adjust animalsPerGroup
      const suggested = Math.floor(availableAnimals / numGroups);
      if (suggested > 0 && suggested !== animalsPerGroup) {
        setAnimalsPerGroup(suggested);
      }
    } else {
      // User edited animalsPerGroup -> auto-adjust numGroups
      const suggested = animalsPerGroup > 0
        ? Math.floor(availableAnimals / animalsPerGroup)
        : 2;
      if (suggested >= 2 && suggested !== numGroups) {
        setNumGroups(Math.min(suggested, 5)); // Cap at 5 groups
      }
    }
  }, [numGroups, animalsPerGroup, availableAnimals, dataset, lastEditedField]);

  // Update individual sex constraint
  const updateSexConstraint = (groupIndex: number, field: 'male_count' | 'female_count', value: number) => {
    setSexConstraints(prev =>
      prev.map((constraint, i) =>
        i === groupIndex ? { ...constraint, [field]: value } : constraint
      )
    );
  };

  // Handle numGroups change with linkage tracking
  const handleNumGroupsChange = (value: number) => {
    setLastEditedField("groups");
    setNumGroups(value);
  };

  // Handle animalsPerGroup change with linkage tracking
  const handleAnimalsPerGroupChange = (value: number) => {
    setLastEditedField("animals");
    setAnimalsPerGroup(value);
  };

  // Handle reserve count changes
  const handleReserveMaleChange = (value: number) => {
    setReserveMaleCount(value);
    // Trigger re-calculation based on last edited field
  };

  const handleReserveFemaleChange = (value: number) => {
    setReserveFemaleCount(value);
    // Trigger re-calculation based on last edited field
  };

  const handleBack = () => {
    setCurrentStep("upload");
  };

  const handleNext = useCallback(() => {
    if (!dataset || selectedIndicators.length === 0) {
      return;
    }

    // Build complete sex constraints: experimental groups + reserve group.
    // The reserve group is only added when it actually holds animals — an empty one
    // would still be counted as a group everywhere downstream (UI, summary, export)
    // and would add a pointless level to the candidate enumeration.
    const hasReserveAnimals = reserveMaleCount + reserveFemaleCount > 0;
    const allConstraints: SexConstraint[] = hasReserveAnimals
      ? [
          ...sexConstraints,
          {
            group_index: numGroups,
            male_count: reserveMaleCount,
            female_count: reserveFemaleCount,
            group_type: "Reserve",
            custom_name: "备用动物",
          },
        ]
      : sexConstraints;

    // Determine group size configuration based on whether distribution is even
    let animalGroupSize: { type: "Uniform"; value: number } | { type: "Custom"; values: number[] };

    if (canDivideEvenly && surplus === 0) {
      // Even distribution: use Uniform
      animalGroupSize = {
        type: "Uniform",
        value: animalsPerGroup,
      };
    } else {
      // Uneven distribution: construct Custom allocation
      const baseSize = Math.floor(availableAnimals / numGroups);
      const remainder = availableAnimals % numGroups;

      // Distribute remainder to last groups
      const customSizes = Array.from({ length: numGroups }, (_, i) =>
        i >= numGroups - remainder ? baseSize + 1 : baseSize
      );

      animalGroupSize = {
        type: "Custom",
        values: customSizes,
      };
    }

    // The method name and the parameters have to agree — the backend rejects, say,
    // "完全随机" carrying an acceptance criterion, because the exported method
    // description would then not match what ran.
    const wantsCriterion =
      method === "ConstrainedRandom" || (method === "BlockedRandom" && criterionOn);
    const acceptance: AcceptanceCriterion | null =
      isRandomized && wantsCriterion
        ? acceptanceTier === "topfraction"
          ? { type: "TopFraction", target_rate: targetRate }
          : { type: "AlphaLine" }
        : null;

    const parsedSeed = seedText.trim() === "" ? null : Number(seedText.trim());
    const randomization: RandomizationConfig | null = usesSeed
      ? {
          seed: parsedSeed !== null && Number.isFinite(parsedSeed) ? parsedSeed : null,
          primary_indicator: method === "BlockedRandom" ? primaryIndicator : null,
          acceptance,
          // 70 indicators under Strict accept roughly one draw in 80; the budget has to
          // cover that case, and each rejected draw is cheap.
          max_attempts: 10000,
          draw_index: 1,
          minimization: isMinimization
            ? {
                covariates,
                allocation_probability: Number(allocationProbabilityText.trim()),
                binning: "Tertiles",
              }
            : null,
        }
      : null;

    // Build group config using complete constraints
    const groupConfig: GroupConfig = {
      num_groups: allConstraints.length,
      animals_per_group: animalGroupSize,
      sex_constraints: allConstraints,
      scenario,
      method,
      randomization,
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
    canDivideEvenly,
    surplus,
    availableAnimals,
    scenario,
    method,
    isRandomized,
    isMinimization,
    usesSeed,
    primaryIndicator,
    seedText,
    acceptanceTier,
    targetRate,
    criterionOn,
    covariates,
    allocationProbabilityText,
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

  // Reserve animals are drawn through the same blocks as the experimental groups, so
  // they carry a quota in every block and the preview has to include them.
  const allConstraintsForPreview: SexConstraint[] =
    reserveMaleCount + reserveFemaleCount > 0
      ? [
          ...sexConstraints,
          {
            group_index: numGroups,
            male_count: reserveMaleCount,
            female_count: reserveFemaleCount,
            group_type: "Reserve",
            custom_name: "备用动物",
          },
        ]
      : sexConstraints;

  // Blocking has nothing to cut blocks along until a primary indicator is chosen, and
  // the software deliberately does not pick one — only the experimenter knows which
  // indicator the study hinges on.
  const needsPrimaryIndicator = method === "BlockedRandom" && primaryIndicator === "";
  const methodImplemented = selectedMethod?.implemented ?? false;

  // Same rule as the primary indicator: the software will not choose what a study
  // balances on. And p has to stay strictly inside (0, 1) — p = 1 would strip the random
  // component out entirely, which is no longer minimization.
  const needsCovariates = isMinimization && covariates.length === 0;
  const parsedProbability = Number(allocationProbabilityText.trim());
  const invalidProbability =
    isMinimization &&
    !(Number.isFinite(parsedProbability) && parsedProbability > 0 && parsedProbability < 1);

  const isValid =
    areSexConstraintsValid &&
    hasSelectedIndicators &&
    !needsPrimaryIndicator &&
    !needsCovariates &&
    !invalidProbability &&
    methodImplemented;

  const scenarioCopy = SCENARIOS.find((s) => s.value === scenario);

  // Block structure preview, computed per sex stratum exactly as the backend does.
  const blockPreview =
    method === "BlockedRandom"
      ? (["male", "female"] as const)
          .map((sex) => {
            const quotas = allConstraintsForPreview.map((c) =>
              sex === "male" ? c.male_count : c.female_count
            );
            return { sex, structure: blockStructure(quotas) };
          })
          .filter((entry) => entry.structure !== null)
      : [];

  return (
    <div className="container max-w-6xl mx-auto py-8 space-y-6">
      {existingRun && (
        <Alert>
          <AlertDescription>
            已有一份计算结果。重新计算后，现有结果和它的全部候选会被替换。
          </AlertDescription>
        </Alert>
      )}

      {/* Scenario — the first decision, and the one that narrows everything else */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Info className="h-5 w-5" />
            应用场景
          </CardTitle>
          <CardDescription>
            先声明这次分组用在什么场景，软件据此推荐分组方式并禁用与之冲突的方法
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="grid grid-cols-3 gap-3">
            {SCENARIOS.map((entry) => (
              <button
                key={entry.value}
                type="button"
                onClick={() => setScenario(entry.value)}
                className={`rounded-lg border-2 p-3 text-left text-sm transition ${
                  scenario === entry.value
                    ? "border-primary bg-primary/5 font-medium"
                    : "border-muted hover:border-muted-foreground/40"
                }`}
              >
                {entry.label}
              </button>
            ))}
          </div>

          {scenarioCopy && (
            <div className="bg-muted/50 rounded-lg p-4 space-y-2 text-sm">
              <p className="text-muted-foreground">{scenarioCopy.description}</p>
              <p>
                <span className="font-medium">推荐：</span>
                {scenarioCopy.recommendation}
              </p>
              <p className="text-muted-foreground">
                <span className="font-medium text-foreground">理由：</span>
                {scenarioCopy.reason}
              </p>
              {scenarioCopy.restriction && (
                <p className="text-amber-700">
                  <span className="font-medium">限制：</span>
                  {scenarioCopy.restriction}
                </p>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      {/* Method */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Dices className="h-5 w-5" />
            分组方式
          </CardTitle>
          <CardDescription>默认方式由场景决定；不可用的方法会标明原因</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {methodNotice && (
            <Alert>
              <Info className="h-4 w-4" />
              <AlertDescription>{methodNotice}</AlertDescription>
            </Alert>
          )}

          <div className="space-y-2">
            {METHODS.map((entry) => {
              const reason = disabledReason(scenario, entry.value);
              const disabled = reason !== null;
              return (
                <label
                  key={entry.value}
                  className={`flex items-start gap-3 rounded-lg border p-3 ${
                    disabled
                      ? "opacity-50 cursor-not-allowed"
                      : "cursor-pointer hover:border-muted-foreground/40"
                  } ${method === entry.value ? "border-primary bg-primary/5" : "border-muted"}`}
                >
                  <input
                    type="radio"
                    className="mt-1 h-4 w-4"
                    disabled={disabled}
                    checked={method === entry.value}
                    onChange={() => {
                      setMethodNotice(null);
                      setMethod(entry.value);
                    }}
                  />
                  <div className="space-y-0.5">
                    <div className="text-sm font-medium">{entry.label}</div>
                    <div className="text-xs text-muted-foreground">{entry.mechanism}</div>
                    {disabled && <div className="text-xs text-amber-700">{reason}</div>}
                  </div>
                </label>
              );
            })}
          </div>

          {method === "BlockedRandom" && (
            <div className="space-y-2">
              <Label>主指标（分层变量）</Label>
              <select
                className="w-full h-9 rounded-md border border-input bg-transparent px-3 text-sm"
                value={primaryIndicator}
                onChange={(e) => setPrimaryIndicator(e.target.value)}
              >
                <option value="">请选择</option>
                {numericIndicators.map((name) => (
                  <option key={name} value={name}>
                    {name}
                  </option>
                ))}
              </select>
              <p className="text-xs text-muted-foreground">
                只列出全部动物均有数值的指标。主指标仅用于排序分块，不参与择优。
              </p>
              {needsPrimaryIndicator && (
                <p className="text-xs text-destructive">请选择主指标后才能提交</p>
              )}

              {blockPreview.length > 0 && primaryIndicator !== "" && (
                <div className="bg-muted/50 rounded-lg p-3 text-sm space-y-1">
                  <div className="font-medium">区组结构</div>
                  {blockPreview.map(({ sex, structure }) => (
                    <div key={sex} className="text-muted-foreground">
                      {blockPreview.length > 1 && (sex === "male" ? "雄性：" : "雌性：")}
                      每 <strong className="text-foreground">{structure!.blockSize}</strong>{" "}
                      只{primaryIndicator}相邻的动物为一个区组，共{" "}
                      <strong className="text-foreground">{structure!.blocks}</strong> 个区组，
                      每区组各组取 {structure!.perBlock.join(" / ")} 只
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

          {isMinimization && (
            <div className="space-y-4">
              <div className="space-y-2">
                <Label>协变量（用于计算不平衡度）</Label>
                <div className="grid grid-cols-2 gap-2 max-h-56 overflow-y-auto rounded-md border p-3">
                  {numericIndicators.map((name) => (
                    <label key={name} className="flex items-center gap-2 text-sm">
                      <Checkbox
                        checked={covariates.includes(name)}
                        onCheckedChange={() =>
                          setCovariates((prev) =>
                            prev.includes(name)
                              ? prev.filter((c) => c !== name)
                              : [...prev, name]
                          )
                        }
                      />
                      <span className="truncate">{name}</span>
                    </label>
                  ))}
                </div>
                <p className="text-xs text-muted-foreground">
                  只列出全部动物均有数值的指标。协变量先按性别分层，层内各取三分位分档，分配时看各档在组间的数量差（已按配额折算）。它和下方「参与统计的指标」是两回事，可以选一样的，也可以不一样。
                </p>
                {needsCovariates && (
                  <p className="text-xs text-destructive">请至少选择一个协变量后才能提交</p>
                )}
                {covariates.length > 3 && (
                  <p className="text-xs text-amber-700">
                    各协变量是等权相加的，选得越多，每个分到的权重越小。一般 1–3 个关键指标就够。
                  </p>
                )}
              </div>

              <div className="space-y-2">
                <Label>分配概率 p</Label>
                <Input
                  type="number"
                  step="0.05"
                  min="0.01"
                  max="0.99"
                  value={allocationProbabilityText}
                  onChange={(e) => setAllocationProbabilityText(e.target.value)}
                />
                <p className="text-xs text-muted-foreground">
                  每只动物以概率 p 分入不平衡度最小的组，否则分入其它组。常用 0.7–0.8。
                </p>
                {invalidProbability && (
                  <p className="text-xs text-destructive">
                    p 要严格落在 0 和 1 之间。p = 1 等于每次都挑最优组，没有随机成分，那就不是最小化法了。
                  </p>
                )}
              </div>
            </div>
          )}

          {usesSeed && (
            <div className="space-y-6">
              <div className="grid grid-cols-2 gap-6">
                <div className="space-y-2">
                  <Label>随机种子</Label>
                  <Input
                    type="number"
                    value={seedText}
                    placeholder="留空则自动生成"
                    onChange={(e) => setSeedText(e.target.value)}
                  />
                  <p className="text-xs text-muted-foreground">
                    种子会随结果一并记录并写入导出文件，用于日后复现同一次分配。
                  </p>
                </div>
              </div>

              {isRandomized && method !== "Random" && (
                <div className="space-y-3">
                  {method === "BlockedRandom" && (
                    <label className="flex items-center gap-2 text-sm">
                      <input
                        type="checkbox"
                        checked={criterionOn}
                        onChange={(e) => setCriterionOn(e.target.checked)}
                      />
                      对其余指标启用接受准则
                    </label>
                  )}
                  {(method === "ConstrainedRandom" || criterionOn) && (
                    <div className="space-y-2">
                      <div className="text-sm font-medium">均衡强度</div>
                      {ACCEPTANCE_TIERS.map((tier) => (
                        <label
                          key={tier.value}
                          className={`block rounded-md border p-3 cursor-pointer ${
                            acceptanceTier === tier.value
                              ? "border-primary bg-primary/5"
                              : "border-muted"
                          }`}
                        >
                          <div className="flex items-center gap-2">
                            <input
                              type="radio"
                              name="acceptance-tier"
                              checked={acceptanceTier === tier.value}
                              onChange={() => setAcceptanceTier(tier.value)}
                            />
                            <span className="font-medium text-sm">{tier.label}</span>
                          </div>
                          <p className="mt-1 text-xs text-muted-foreground">
                            {tier.description}
                          </p>
                        </label>
                      ))}
                      {acceptanceTier === "topfraction" && (
                        <div className="flex items-center gap-2 text-sm">
                          <span>目标接受率：</span>
                          {TARGET_RATE_PRESETS.map((rate) => (
                            <Button
                              key={rate}
                              type="button"
                              size="sm"
                              variant={targetRate === rate ? "default" : "outline"}
                              onClick={() => setTargetRate(rate)}
                            >
                              {Math.round(rate * 100)}%
                            </Button>
                          ))}
                        </div>
                      )}
                      <p className="text-xs text-muted-foreground">{ACCEPTANCE_FOOTNOTE}</p>
                    </div>
                  )}
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

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
                onChange={(e) => handleNumGroupsChange(Number(e.target.value))}
                min={2}
                max={5}
              />
            </div>
            <div className="space-y-2">
              <Label>每组动物数</Label>
              <Input
                type="number"
                value={animalsPerGroup}
                onChange={(e) => handleAnimalsPerGroupChange(Number(e.target.value))}
                min={2}
              />
            </div>
          </div>

          {/* Configuration Status Display */}
          <div className="bg-muted/50 rounded-lg p-4 space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">实验组可用动物数:</span>
              <span className="font-semibold">{availableAnimals} 只</span>
            </div>
            <div className="flex items-center justify-between text-sm">
              <span className="text-muted-foreground">当前配置需要:</span>
              <span className="font-semibold">{requiredAnimals} 只</span>
            </div>
            {!canDivideEvenly && surplus === 0 && (
              <div className="flex items-center justify-between text-sm">
                <span className="text-muted-foreground">分配方案:</span>
                <span className="font-semibold text-amber-600">
                  {Array.from({ length: numGroups }, (_, i) => {
                    const baseSize = Math.floor(availableAnimals / numGroups);
                    const remainder = availableAnimals % numGroups;
                    return i >= numGroups - remainder ? baseSize + 1 : baseSize;
                  }).join(" + ")} 只/组
                </span>
              </div>
            )}
          </div>

          {/* Smart Suggestions */}
          {hasConflict && (
            <Alert variant={surplus > 0 ? "default" : "destructive"}>
              <Info className="h-4 w-4" />
              <AlertDescription>
                {surplus > 0 ? (
                  <div className="space-y-1">
                    <div className="font-medium">配置提示</div>
                    <div className="text-sm">
                      当前配置将剩余 <strong>{surplus}</strong> 只动物。
                      建议设置备用组 <strong>{surplus}</strong> 只，或调整分组参数。
                    </div>
                  </div>
                ) : (
                  <div className="space-y-1">
                    <div className="font-medium">配置错误</div>
                    <div className="text-sm">
                      当前配置需要 <strong>{requiredAnimals}</strong> 只动物，
                      但实际只有 <strong>{availableAnimals}</strong> 只可用。
                      请调整分组参数。
                    </div>
                  </div>
                )}
              </AlertDescription>
            </Alert>
          )}

          {!canDivideEvenly && surplus === 0 && (
            <Alert>
              <Info className="h-4 w-4" />
              <AlertDescription>
                <div className="space-y-1">
                  <div className="font-medium">不均等分组提示</div>
                  <div className="text-sm">
                    当前配置无法均等分组，将采用不均等方案。
                    {reserveMaleCount + reserveFemaleCount === 0 && (
                      <span className="block mt-1">
                        建议设置备用组 <strong>{availableAnimals % numGroups}</strong> 只，
                        使实验组能够均等分配为 <strong>{Math.floor(availableAnimals / numGroups)}</strong> 只/组。
                      </span>
                    )}
                  </div>
                </div>
              </AlertDescription>
            </Alert>
          )}

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
                  <CardDescription className="text-xs">
                    实验组可用: <strong className="text-foreground">{availableAnimals}</strong> 只
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3">
                  <div className="space-y-2">
                    <Label className="text-xs flex items-center gap-1.5">
                      <span className="text-blue-600 font-semibold text-base">♂</span>
                      雄性数量
                    </Label>
                    <Input
                      type="number"
                      value={reserveMaleCount}
                      onChange={(e) => handleReserveMaleChange(Number(e.target.value))}
                      min={0}
                      max={dataset.metadata.male_count}
                    />
                  </div>
                  <div className="space-y-2">
                    <Label className="text-xs flex items-center gap-1.5">
                      <span className="text-pink-600 font-semibold text-base">♀</span>
                      雌性数量
                    </Label>
                    <Input
                      type="number"
                      value={reserveFemaleCount}
                      onChange={(e) => handleReserveFemaleChange(Number(e.target.value))}
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
                      <Label className="text-xs flex items-center gap-1.5">
                        <span className="text-blue-600 font-semibold text-base">♂</span>
                        雄性数量
                      </Label>
                      <Input
                        type="number"
                        value={constraint.male_count}
                        onChange={(e) => updateSexConstraint(index, 'male_count', Number(e.target.value))}
                        min={0}
                      />
                    </div>
                    <div className="space-y-2">
                      <Label className="text-xs flex items-center gap-1.5">
                        <span className="text-pink-600 font-semibold text-base">♀</span>
                        雌性数量
                      </Label>
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
