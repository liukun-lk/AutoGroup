import { useMemo, useState } from "react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { CheckCircle2, ChevronDown, ChevronRight, XCircle } from "lucide-react";
import type { IndicatorStats } from "@/types";

/**
 * Pairwise post-hoc comparisons, read from coarse to fine.
 *
 * The flat one-row-per-comparison table this replaces grew as C(groups, 2) x indicators —
 * 90 rows for 10 groups and 2 indicators, 210 for 3 groups and 70 — while carrying almost
 * no information: a candidate is only ever returned when *every* pairwise comparison
 * clears alpha, so under Strict mode all of those rows are guaranteed to read "ns".
 *
 * So the verdict comes first, then the margin, and only then the numbers. The matrix is
 * not a summary: its lower triangle holds all C(groups, 2) comparisons, so nothing is
 * hidden — it just restores the two-dimensional shape the flat list had flattened away.
 */

interface Props {
  statistics: IndicatorStats[];
  alpha: number;
  /** Display name for a group id, e.g. `第 3 组`. */
  labelOf: (groupId: number) => string;
}

interface Comparison {
  a: number;
  b: number;
  p: number;
  valid: boolean;
}

interface IndicatorMatrix {
  name: string;
  method: string;
  /** Group ids that actually appear, ascending. Reserve groups never do. */
  groupIds: number[];
  cells: Map<string, Comparison>;
  comparisons: Comparison[];
  worst: Comparison;
  failing: Comparison[];
}

const key = (a: number, b: number) => `${Math.min(a, b)}-${Math.max(a, b)}`;

/**
 * Distance from the decision threshold, in four bands. Defined relative to alpha rather
 * than at fixed cutoffs, because alpha is configurable: at alpha = 0.01 a P of 0.03 is
 * comfortable, at alpha = 0.05 it is one bad draw away from failing.
 */
function band(p: number, alpha: number): "fail" | "close" | "ok" | "safe" {
  if (p <= alpha) return "fail";
  if (p <= alpha * 4) return "close";
  if (p <= 0.5) return "ok";
  return "safe";
}

const BAND_STYLE: Record<string, string> = {
  fail: "bg-red-100 text-red-800",
  close: "bg-amber-100 text-amber-800",
  ok: "bg-emerald-50 text-emerald-700",
  safe: "bg-emerald-100 text-emerald-800",
};

export function PosthocComparisons({ statistics, alpha, labelOf }: Props) {
  const matrices = useMemo<IndicatorMatrix[]>(() => {
    return statistics
      .filter((stat) => (stat.posthoc_results?.length ?? 0) > 0)
      .map((stat) => {
        const comparisons: Comparison[] = (stat.posthoc_results ?? []).map((c) => ({
          a: c.group1_id,
          b: c.group2_id,
          p: c.p_value,
          valid: c.is_valid,
        }));

        const groupIds = [...new Set(comparisons.flatMap((c) => [c.a, c.b]))].sort(
          (x, y) => x - y
        );

        const cells = new Map(comparisons.map((c) => [key(c.a, c.b), c]));
        const worst = comparisons.reduce((lo, c) => (c.p < lo.p ? c : lo));

        return {
          name: stat.indicator_name,
          method: stat.test_method,
          groupIds,
          cells,
          comparisons,
          worst,
          failing: comparisons.filter((c) => !c.valid),
        };
      });
  }, [statistics]);

  const total = matrices.reduce((sum, m) => sum + m.comparisons.length, 0);
  const failing = matrices.flatMap((m) => m.failing.map((c) => ({ indicator: m.name, ...c })));
  const closest = matrices.reduce<{ indicator: string; c: Comparison } | null>(
    (lo, m) => (lo === null || m.worst.p < lo.c.p ? { indicator: m.name, c: m.worst } : lo),
    null
  );

  // Anything that failed is open on arrival; the rest start collapsed, because under
  // Strict mode they are all guaranteed to pass and there is nothing to look for.
  const [expanded, setExpanded] = useState<Set<string>>(
    () => new Set(matrices.filter((m) => m.failing.length > 0).map((m) => m.name))
  );

  if (matrices.length === 0) return null;

  const toggle = (name: string) =>
    setExpanded((prev) => {
      const next = new Set(prev);
      if (!next.delete(name)) next.add(name);
      return next;
    });

  const allOpen = expanded.size === matrices.length;

  return (
    <Card>
      <CardHeader>
        <CardTitle>组间两两比较</CardTitle>
        <CardDescription>
          整体差异检验之外，每一对实验组之间的事后检验 P 值，共 {total} 组比较。
          矩阵下三角即为全部比较，没有省略。
        </CardDescription>
      </CardHeader>

      <CardContent className="space-y-4">
        {/* Layer 1: the verdict, and how close the closest call was. */}
        <div
          className={`rounded-lg border p-4 ${
            failing.length === 0
              ? "border-emerald-200 bg-emerald-50"
              : "border-red-200 bg-red-50"
          }`}
        >
          <div className="flex items-start gap-3">
            {failing.length === 0 ? (
              <CheckCircle2 className="h-5 w-5 text-emerald-600 mt-0.5 shrink-0" />
            ) : (
              <XCircle className="h-5 w-5 text-red-600 mt-0.5 shrink-0" />
            )}
            <div className="space-y-1">
              <div className="font-medium">
                {failing.length === 0
                  ? `${total} 组两两比较全部通过`
                  : `${total} 组中 ${failing.length} 组未通过`}
                <span className="ml-2 font-normal text-muted-foreground text-sm">
                  α = {alpha}
                </span>
              </div>

              {closest && (
                <button
                  type="button"
                  onClick={() => toggle(closest.indicator)}
                  className="text-sm text-left hover:underline"
                >
                  <span className="text-muted-foreground">最接近的一对：</span>
                  <span className="font-medium">{closest.indicator}</span>
                  <span className="mx-1 text-muted-foreground">
                    {labelOf(closest.c.a)} vs {labelOf(closest.c.b)}
                  </span>
                  <span className="font-mono font-medium">P = {closest.c.p.toFixed(4)}</span>
                </button>
              )}
            </div>
          </div>

          {failing.length > 0 && (
            <div className="mt-3 space-y-1 border-t border-red-200 pt-3 text-sm">
              {failing.map((c) => (
                <div key={`${c.indicator}-${key(c.a, c.b)}`} className="flex gap-3">
                  <span className="font-medium">{c.indicator}</span>
                  <span className="text-muted-foreground">
                    {labelOf(c.a)} vs {labelOf(c.b)}
                  </span>
                  <span className="font-mono text-red-700">P = {c.p.toFixed(4)}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="flex items-center justify-between">
          <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
            <span>底色：</span>
            <LegendChip band="safe">P &gt; 0.5</LegendChip>
            <LegendChip band="ok">
              {(alpha * 4).toFixed(2)} &lt; P ≤ 0.5
            </LegendChip>
            <LegendChip band="close">
              {alpha} &lt; P ≤ {(alpha * 4).toFixed(2)}
            </LegendChip>
            <LegendChip band="fail">P ≤ {alpha}</LegendChip>
          </div>

          <Button
            variant="outline"
            size="sm"
            onClick={() =>
              setExpanded(allOpen ? new Set() : new Set(matrices.map((m) => m.name)))
            }
          >
            {allOpen ? "全部收起" : "全部展开"}
          </Button>
        </div>

        {/* Layer 2 + 3: one collapsible section per indicator, opening into its matrix. */}
        <div className="rounded-md border divide-y">
          {matrices.map((m) => {
            const isOpen = expanded.has(m.name);
            return (
              <div key={m.name}>
                <button
                  type="button"
                  onClick={() => toggle(m.name)}
                  className="flex w-full items-center gap-3 px-4 py-3 text-left hover:bg-muted/50"
                >
                  {isOpen ? (
                    <ChevronDown className="h-4 w-4 shrink-0 text-muted-foreground" />
                  ) : (
                    <ChevronRight className="h-4 w-4 shrink-0 text-muted-foreground" />
                  )}

                  <span className="font-medium w-40 shrink-0 truncate">{m.name}</span>
                  <span className="text-xs text-muted-foreground w-56 shrink-0 truncate">
                    {m.method}
                  </span>
                  <span className="text-sm text-muted-foreground">
                    {m.comparisons.length} 对
                  </span>

                  <span className="ml-auto flex items-center gap-3 text-sm">
                    <span className="text-muted-foreground">
                      最严格 {labelOf(m.worst.a)} vs {labelOf(m.worst.b)}
                    </span>
                    <span className="font-mono font-medium">{m.worst.p.toFixed(4)}</span>
                    <span
                      className={`inline-flex items-center rounded-full px-2 py-1 text-xs font-medium ${
                        m.failing.length === 0
                          ? "bg-emerald-100 text-emerald-700"
                          : "bg-red-100 text-red-700"
                      }`}
                    >
                      {m.failing.length === 0 ? "全部通过" : `${m.failing.length} 对未通过`}
                    </span>
                  </span>
                </button>

                {isOpen && (
                  <div className="overflow-x-auto px-4 pb-4">
                    <Matrix matrix={m} alpha={alpha} labelOf={labelOf} />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}

function LegendChip({ band, children }: { band: string; children: React.ReactNode }) {
  return (
    <span className="inline-flex items-center gap-1">
      <span className={`inline-block h-3 w-3 rounded-sm ${BAND_STYLE[band]}`} />
      {children}
    </span>
  );
}

/**
 * Lower triangle only: a comparison of group i with group j is the same comparison as j
 * with i, so a full square would print every P value twice.
 */
function Matrix({
  matrix,
  alpha,
  labelOf,
}: {
  matrix: IndicatorMatrix;
  alpha: number;
  labelOf: (groupId: number) => string;
}) {
  const rows = matrix.groupIds.slice(1);
  const cols = matrix.groupIds.slice(0, -1);

  // Group labels run long ("第 10 组"); the axis needs the short form to stay readable at
  // ten groups, and the full name is one hover away.
  const short = (groupId: number) => labelOf(groupId).replace(/^第\s*(\d+)\s*组$/, "组$1");

  return (
    <table className="border-separate border-spacing-0.5 text-xs">
      <thead>
        <tr>
          <th className="w-14" />
          {cols.map((col) => (
            <th
              key={col}
              className="px-2 py-1 font-medium text-muted-foreground"
              title={labelOf(col)}
            >
              {short(col)}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr key={row}>
            <th
              className="pr-2 text-right font-medium text-muted-foreground"
              title={labelOf(row)}
            >
              {short(row)}
            </th>
            {cols.map((col) => {
              if (col >= row) return <td key={col} />;

              const cell = matrix.cells.get(key(row, col));
              if (!cell) return <td key={col} />;

              const isWorst = key(row, col) === key(matrix.worst.a, matrix.worst.b);

              return (
                <td key={col}>
                  <div
                    title={`${labelOf(row)} vs ${labelOf(col)}　P = ${cell.p.toFixed(6)}`}
                    className={`rounded px-2 py-1.5 text-center font-mono tabular-nums ${
                      BAND_STYLE[band(cell.p, alpha)]
                    } ${isWorst ? "ring-2 ring-offset-1 ring-slate-400" : ""}`}
                  >
                    {cell.p.toFixed(3)}
                  </div>
                </td>
              );
            })}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
