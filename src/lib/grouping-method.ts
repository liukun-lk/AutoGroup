/**
 * Scenario and method metadata for the configuration page.
 *
 * The ordering here is deliberate: the user declares what the study is for, and the
 * software narrows the methods from there. Five randomization variants presented flat
 * are a choice nobody can make well, and under a GLP submission the wrong one only
 * surfaces at review.
 */

import type { GroupingMethod, StudyScenario } from "@/types";

export interface ScenarioCopy {
  value: StudyScenario;
  label: string;
  description: string;
  recommendation: string;
  reason: string;
  /** Shown only when the scenario forbids something. */
  restriction?: string;
}

export const SCENARIOS: ScenarioCopy[] = [
  {
    value: "GlpSubmission",
    label: "GLP 申报实验",
    description:
      "用于向监管机构提交的非临床安全性研究。分组必须采用标准、可验证的随机化方法，并完整记录随机种子与执行过程，以备审计与 QA 检查。",
    recommendation: "按体重等关键指标分层的区组随机化。",
    reason: "监管机构最熟悉这套方法，各组样本量与关键指标的均衡由设计保证，审评风险最小。",
    restriction:
      "本场景禁用统计均衡优化。它按 P 值择优挑选分组，不属于随机化，写进申报材料与实际方法不符。",
  },
  {
    value: "ConfirmatoryTrial",
    label: "确证性临床试验",
    description: "样本量有限，需要平衡的基线协变量又多，单纯随机化很难同时兼顾。",
    recommendation: "最小化法（协变量自适应随机化）。",
    reason:
      "逐一分配受试者，每次把新个体分到使各协变量总体不平衡最小的那一组，同时保留随机成分。小样本下的均衡效果明显好于简单随机，监管机构已逐步接受。",
    restriction:
      "序贯最小化尚未实现。现有的统计均衡优化与它目标相同、机制不同（全局择优搜索，无不平衡度量、无随机成分），导出仍按非随机标注。",
  },
  {
    value: "Exploratory",
    label: "探索性 / 非 GLP 实验",
    description: "内部摸索性研究，不用于申报。",
    recommendation: "完全随机或分层随机；特别看重基线均衡时可用统计均衡优化。",
    reason: "没有监管口径的约束，可以按研究目的自由取舍。要均衡就用搜索式优化，要分配机制干净就用随机化。",
    restriction: "统计均衡优化的结果不能用于申报材料，导出文件会如实标注其分组原理。",
  },
];

export interface MethodCopy {
  value: GroupingMethod;
  label: string;
  /** One line on the allocation mechanism, so the name is never shown on its own. */
  mechanism: string;
  requiresPrimaryIndicator: boolean;
  implemented: boolean;
}

export const METHODS: MethodCopy[] = [
  {
    value: "BlockedRandom",
    label: "按主指标分层随机",
    mechanism: "按性别与主指标分层，层内洗牌后按配额发牌；主指标的均衡由构造保证",
    requiresPrimaryIndicator: true,
    implemented: true,
  },
  {
    value: "Random",
    label: "完全随机",
    mechanism: "种子化洗牌后按配额分配，不读取任何指标值",
    requiresPrimaryIndicator: false,
    implemented: true,
  },
  {
    value: "ConstrainedRandom",
    label: "受限随机化",
    mechanism: "完全随机加基线均衡接受准则，不达标则按同一随机序列重抽",
    requiresPrimaryIndicator: false,
    implemented: true,
  },
  {
    value: "Minimization",
    label: "最小化法",
    mechanism: "逐只分配，每次分给使不平衡度最小的组（规划中，尚未实现）",
    requiresPrimaryIndicator: false,
    implemented: false,
  },
  {
    value: "Optimized",
    label: "统计均衡优化",
    mechanism: "枚举或采样全部候选划分，按 min(P) 与 mean(P) 择优——不是随机化",
    requiresPrimaryIndicator: false,
    implemented: true,
  },
];

/**
 * Acceptance-tier copy for the randomized methods' rejection-sampling rule. Verbatim from
 * the design doc §2.3 — do not edit the Chinese text.
 */
export interface AcceptanceTierCopy {
  value: "alpha" | "topfraction";
  label: string;
  description: string;
}

export const ACCEPTANCE_TIERS: AcceptanceTierCopy[] = [
  {
    value: "alpha",
    label: "基础档——排除可检出差异的分组",
    description:
      "每一签都检验全部所选指标，任何一个指标 P ≤ α 就废签重抽。只排除统计上能检出差异的约一成分组，其余一律等概率接受。均衡程度与普通随机接近，随机性保留最足。适合「不出最差情况即可」的研究。",
  },
  {
    value: "topfraction",
    label: "增强档——只接受最均衡的前 X%",
    description:
      "软件先在本数据上做 1000 次种子化模拟，定出「最均衡的前 X%」对应的门槛（按全部所选指标中最差的那个 P 值），再正式抽签，达不到门槛就废签重抽。全部指标一视同仁，没有主次之分。X 越小分得越匀、自动重抽越多（通常仍在毫秒级）。门槛与定标过程会写入导出文件，作为预先声明的接受准则。",
  },
];

export const ACCEPTANCE_FOOTNOTE =
  "两档都是抽签之前定死、由软件自动执行的规则，属于受限随机化；不构成看结果择优。";

export const TARGET_RATE_PRESETS = [0.1, 0.25, 0.5];

/** Mirrors `StudyScenario::allows` in the backend, which is the authority. */
export function isMethodAllowed(scenario: StudyScenario, method: GroupingMethod): boolean {
  return !(scenario === "GlpSubmission" && method === "Optimized");
}

export function disabledReason(scenario: StudyScenario, method: GroupingMethod): string | null {
  if (!METHODS.find((m) => m.value === method)?.implemented) {
    return "规划中，尚未实现";
  }
  if (!isMethodAllowed(scenario, method)) {
    return "GLP 申报场景禁用：按 P 值择优挑选分组不属于随机化，与申报表述不符";
  }
  return null;
}

export function defaultMethodFor(scenario: StudyScenario): GroupingMethod {
  switch (scenario) {
    case "GlpSubmission":
      return "BlockedRandom";
    case "ConfirmatoryTrial":
      return "Optimized";
    default:
      return "Random";
  }
}

function gcd(a: number, b: number): number {
  return b === 0 ? a : gcd(b, a % b);
}

/**
 * Block structure for a set of group quotas, matching `build_plan` in the backend: the
 * block count is the gcd of the quotas, so each block hands out `quota / blocks` animals
 * to each group.
 */
export function blockStructure(quotas: number[]): {
  blocks: number;
  blockSize: number;
  perBlock: number[];
} | null {
  const total = quotas.reduce((sum, q) => sum + q, 0);
  if (total === 0) return null;

  const blocks = Math.max(quotas.reduce(gcd, 0), 1);
  return {
    blocks,
    blockSize: total / blocks,
    perBlock: quotas.map((q) => q / blocks),
  };
}
