#!/usr/bin/env python3
"""AutoGroup reference grouping engine: exact statistics, zero dependencies.

This mirrors the production Rust engine (src-tauri/src/core/grouping +
src-tauri/src/core/stats) but computes every p-value from textbook-exact
distributions, so it can be used as an oracle: the Rust engine is fast, this
one is right. When the two disagree, this file is the tie-breaker.

Only the Python standard library is used (math/itertools/zipfile/xml), so it
runs anywhere without pip installs.

Subcommands
    group       enumerate candidate groupings, rank them, print Top-N
    verify      recompute exact statistics for one fixed assignment
    self-test   validate the statistical kernels against analytic identities,
                published critical values, and Monte-Carlo simulation

Run `python3 grouping_engine.py <subcommand> --help` for arguments.
"""

from __future__ import annotations

import argparse
import itertools
import json
import math
import random
import re
import sys
import zipfile
import xml.etree.ElementTree as ET
from dataclasses import dataclass, field
from functools import lru_cache
from typing import Callable, Iterable, Sequence

# ---------------------------------------------------------------------------
# Section 1. Exact distribution functions
#
# Everything downstream depends on these four primitives, so they are written
# to be verifiable in isolation (see the `self-test` subcommand):
#   * regularized incomplete beta  -> Student t and Fisher-Snedecor tails
#   * studentized range            -> Tukey HSD
#   * studentized maximum modulus  -> Dunnett's T3
# ---------------------------------------------------------------------------

_SQRT2 = math.sqrt(2.0)
_INV_SQRT_2PI = 1.0 / math.sqrt(2.0 * math.pi)


def norm_cdf(z: float) -> float:
    return 0.5 * math.erfc(-z / _SQRT2)


def norm_pdf(z: float) -> float:
    return _INV_SQRT_2PI * math.exp(-0.5 * z * z)


def _betacf(a: float, b: float, x: float) -> float:
    """Continued fraction for the incomplete beta function (Lentz's method)."""
    tiny = 1e-300
    qab, qap, qam = a + b, a + 1.0, a - 1.0
    c = 1.0
    d = 1.0 - qab * x / qap
    if abs(d) < tiny:
        d = tiny
    d = 1.0 / d
    h = d
    for m in range(1, 401):
        m2 = 2 * m
        aa = m * (b - m) * x / ((qam + m2) * (a + m2))
        d = 1.0 + aa * d
        if abs(d) < tiny:
            d = tiny
        c = 1.0 + aa / c
        if abs(c) < tiny:
            c = tiny
        d = 1.0 / d
        h *= d * c
        aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2))
        d = 1.0 + aa * d
        if abs(d) < tiny:
            d = tiny
        c = 1.0 + aa / c
        if abs(c) < tiny:
            c = tiny
        d = 1.0 / d
        delta = d * c
        h *= delta
        if abs(delta - 1.0) < 1e-15:
            break
    return h


def betainc(a: float, b: float, x: float) -> float:
    """Regularized incomplete beta I_x(a, b)."""
    if x <= 0.0:
        return 0.0
    if x >= 1.0:
        return 1.0
    log_front = (
        math.lgamma(a + b)
        - math.lgamma(a)
        - math.lgamma(b)
        + a * math.log(x)
        + b * math.log1p(-x)
    )
    if x < (a + 1.0) / (a + b + 2.0):
        return math.exp(log_front) * _betacf(a, b, x) / a
    return 1.0 - math.exp(log_front) * _betacf(b, a, 1.0 - x) / b


def t_sf_two_sided(t: float, df: float) -> float:
    """Two-tailed p-value of Student's t with `df` degrees of freedom."""
    if df <= 0 or not math.isfinite(t):
        return float("nan")
    t = abs(t)
    if t == 0.0:
        return 1.0
    return betainc(df / 2.0, 0.5, df / (df + t * t))


def f_sf(f: float, df1: float, df2: float) -> float:
    """Upper tail of the F distribution: P(F_{df1,df2} > f)."""
    if df1 <= 0 or df2 <= 0 or not math.isfinite(f):
        return float("nan")
    if f <= 0.0:
        return 1.0
    return betainc(df2 / 2.0, df1 / 2.0, df2 / (df2 + df1 * f))


# --- Gauss-Legendre quadrature ---------------------------------------------


def _legendre(n: int, x: float) -> tuple[float, float]:
    p_prev, p_curr = 1.0, x
    for j in range(2, n + 1):
        p_prev, p_curr = p_curr, ((2 * j - 1) * x * p_curr - (j - 1) * p_prev) / j
    dp = n * (x * p_curr - p_prev) / (x * x - 1.0)
    return p_curr, dp


@lru_cache(maxsize=None)
def _gauss_legendre(n: int) -> tuple[tuple[float, ...], tuple[float, ...]]:
    nodes, weights = [], []
    for i in range(1, n + 1):
        x = math.cos(math.pi * (i - 0.25) / (n + 0.5))
        for _ in range(100):
            p, dp = _legendre(n, x)
            dx = -p / dp
            x += dx
            if abs(dx) < 1e-15:
                break
        _, dp = _legendre(n, x)
        nodes.append(x)
        weights.append(2.0 / ((1.0 - x * x) * dp * dp))
    return tuple(nodes), tuple(weights)


def _integrate(
    func: Callable[[float], float], a: float, b: float, panels: int, nodes: int = 12
) -> float:
    """Composite Gauss-Legendre integration of a smooth integrand."""
    if b <= a:
        return 0.0
    xs, ws = _gauss_legendre(nodes)
    h = (b - a) / panels
    half = h / 2.0
    total = 0.0
    for p in range(panels):
        mid = a + h * p + half
        acc = 0.0
        for x, w in zip(xs, ws):
            acc += w * func(mid + half * x)
        total += acc
    return total * half


def _chi_scale_integral(func: Callable[[float], float], nu: float) -> float:
    """Integrate func(s) against the density of s = sqrt(chi2_nu / nu).

    Tukey HSD and Dunnett's T3 both average a normal-theory probability over
    the sampling distribution of the pooled standard deviation; this helper is
    that average.
    """
    log_c = 0.5 * nu * math.log(nu) - math.lgamma(nu / 2.0) - (nu / 2.0 - 1.0) * math.log(2.0)

    def integrand(s: float) -> float:
        if s <= 0.0:
            return 0.0
        log_dens = log_c + (nu - 1.0) * math.log(s) - nu * s * s / 2.0
        if log_dens < -700.0:
            return 0.0
        return math.exp(log_dens) * func(s)

    spread = 12.0 / math.sqrt(nu)
    lo = max(0.0, 1.0 - spread)
    hi = 1.0 + spread
    return _integrate(integrand, lo, hi, panels=16, nodes=12)


# --- Studentized range (Tukey HSD) -----------------------------------------


def _range_prob(w: float, k: int) -> float:
    """P(range of k iid N(0,1) <= w)."""
    if w <= 0.0:
        return 0.0

    def integrand(z: float) -> float:
        d = norm_cdf(z) - norm_cdf(z - w)
        if d <= 0.0:
            return 0.0
        return norm_pdf(z) * d ** (k - 1)

    lo, hi = -8.5, 8.5 + w
    panels = max(20, int(hi - lo))
    return k * _integrate(integrand, lo, hi, panels=panels, nodes=12)


class _RangeProbTable:
    """Tabulated `_range_prob` for one k, with 4-point Lagrange interpolation.

    The studentized range p-value is a double integral. The inner integral
    depends only on (w, k), so tabulating it once per k turns each p-value into
    a handful of table lookups - that is what keeps exact Tukey p-values
    affordable in pure Python.
    """

    STEP = 0.05
    MAX_W = 32.0

    def __init__(self, k: int) -> None:
        self.k = k
        self.n = int(self.MAX_W / self.STEP) + 1
        self.values = [_range_prob(i * self.STEP, k) for i in range(self.n)]

    def __call__(self, w: float) -> float:
        if w <= 0.0:
            return 0.0
        if w >= self.MAX_W:
            return 1.0
        pos = w / self.STEP
        i = min(max(int(pos) - 1, 0), self.n - 4)
        t = pos - i
        y0, y1, y2, y3 = self.values[i : i + 4]
        # Lagrange interpolation on the uniform nodes 0,1,2,3.
        p = (
            -y0 * (t - 1) * (t - 2) * (t - 3) / 6.0
            + y1 * t * (t - 2) * (t - 3) / 2.0
            - y2 * t * (t - 1) * (t - 3) / 2.0
            + y3 * t * (t - 1) * (t - 2) / 6.0
        )
        return min(1.0, max(0.0, p))


@lru_cache(maxsize=None)
def _range_table(k: int) -> _RangeProbTable:
    return _RangeProbTable(k)


def srange_sf(q: float, k: int, nu: float) -> float:
    """Upper tail of the studentized range distribution."""
    if q <= 0.0:
        return 1.0
    table = _range_table(k)
    cdf = _chi_scale_integral(lambda s: table(q * s), nu)
    return min(1.0, max(0.0, 1.0 - cdf))


@lru_cache(maxsize=None)
def srange_crit(alpha: float, k: int, nu: float) -> float:
    """q such that srange_sf(q, k, nu) == alpha (monotone bisection)."""
    lo, hi = 0.0, 20.0
    while srange_sf(hi, k, nu) > alpha and hi < 400.0:
        hi *= 2.0
    for _ in range(80):
        mid = (lo + hi) / 2.0
        if srange_sf(mid, k, nu) > alpha:
            lo = mid
        else:
            hi = mid
        if hi - lo < 1e-9:
            break
    return (lo + hi) / 2.0


# --- Studentized maximum modulus (Dunnett's T3) ----------------------------


def smm_sf(m: float, c: int, nu: float) -> float:
    """Upper tail of the studentized maximum modulus with c comparisons."""
    if m <= 0.0:
        return 1.0
    if c <= 0:
        return float("nan")

    def inner(s: float) -> float:
        base = 2.0 * norm_cdf(m * s) - 1.0
        if base <= 0.0:
            return 0.0
        return base**c

    cdf = _chi_scale_integral(inner, nu)
    return min(1.0, max(0.0, 1.0 - cdf))


# ---------------------------------------------------------------------------
# Section 2. Statistical tests
#
# Signatures take a list of groups (each a list of floats) and return
# p-values, mirroring src-tauri/src/core/stats.
# ---------------------------------------------------------------------------


def mean(xs: Sequence[float]) -> float:
    return math.fsum(xs) / len(xs)


def variance(xs: Sequence[float]) -> float:
    """Unbiased (n-1) sample variance."""
    m = mean(xs)
    return math.fsum((x - m) ** 2 for x in xs) / (len(xs) - 1)


def median(xs: Sequence[float]) -> float:
    s = sorted(xs)
    n = len(s)
    mid = n // 2
    return s[mid] if n % 2 else (s[mid - 1] + s[mid]) / 2.0


def one_way_anova(groups: Sequence[Sequence[float]]) -> float:
    """Classic one-way ANOVA p-value."""
    k = len(groups)
    n_total = sum(len(g) for g in groups)
    if k < 2 or n_total <= k:
        return float("nan")
    grand = math.fsum(math.fsum(g) for g in groups) / n_total
    means = [mean(g) for g in groups]
    ssb = math.fsum(len(g) * (m - grand) ** 2 for g, m in zip(groups, means))
    ssw = math.fsum(math.fsum((x - m) ** 2 for x in g) for g, m in zip(groups, means))
    df_b, df_w = k - 1.0, float(n_total - k)
    if ssw <= 0.0:
        # Zero within-group variance: means either coincide (p=1) or not (p=0).
        return 1.0 if ssb <= 0.0 else 0.0
    f_stat = (ssb / df_b) / (ssw / df_w)
    return f_sf(f_stat, df_b, df_w)


def welch_anova(groups: Sequence[Sequence[float]]) -> float:
    """Welch's heteroscedastic ANOVA p-value."""
    k = len(groups)
    if k < 2:
        return float("nan")
    stats = []
    for g in groups:
        n = float(len(g))
        v = variance(g)
        if v <= 0.0:
            return float("nan")  # weight = n/v undefined
        stats.append((n, mean(g), v, n / v))
    sum_w = math.fsum(s[3] for s in stats)
    grand = math.fsum(s[3] * s[1] for s in stats) / sum_w
    numerator = math.fsum(s[3] * (s[1] - grand) ** 2 for s in stats) / (k - 1.0)
    h = math.fsum((1.0 - s[3] / sum_w) ** 2 / (s[0] - 1.0) for s in stats)
    denominator = 1.0 + (2.0 * (k - 2.0) / (k * k - 1.0)) * h
    f_stat = numerator / denominator
    df1 = k - 1.0
    df2 = (k * k - 1.0) / (3.0 * h)
    return f_sf(f_stat, df1, df2)


def levene(groups: Sequence[Sequence[float]], center: str = "mean") -> float:
    """Levene's test for homogeneity of variance.

    center="mean" reproduces the Rust engine; center="median" is the
    Brown-Forsythe variant (scipy's default), which is more robust to
    skewed indicators.
    """
    centers = [mean(g) if center == "mean" else median(g) for g in groups]
    transformed = [[abs(x - c) for x in g] for g, c in zip(groups, centers)]
    return one_way_anova(transformed)


def student_ttest(a: Sequence[float], b: Sequence[float]) -> float:
    n1, n2 = float(len(a)), float(len(b))
    v1, v2 = variance(a), variance(b)
    pooled = ((n1 - 1.0) * v1 + (n2 - 1.0) * v2) / (n1 + n2 - 2.0)
    se = math.sqrt(pooled * (1.0 / n1 + 1.0 / n2))
    if se <= 0.0:
        return 1.0 if mean(a) == mean(b) else 0.0
    return t_sf_two_sided((mean(a) - mean(b)) / se, n1 + n2 - 2.0)


def welch_df(v1: float, n1: float, v2: float, n2: float) -> float:
    num = (v1 / n1 + v2 / n2) ** 2
    den = (v1 / n1) ** 2 / (n1 - 1.0) + (v2 / n2) ** 2 / (n2 - 1.0)
    return num / den


def welch_ttest(a: Sequence[float], b: Sequence[float]) -> float:
    n1, n2 = float(len(a)), float(len(b))
    v1, v2 = variance(a), variance(b)
    se = math.sqrt(v1 / n1 + v2 / n2)
    if se <= 0.0:
        return 1.0 if mean(a) == mean(b) else 0.0
    return t_sf_two_sided((mean(a) - mean(b)) / se, welch_df(v1, n1, v2, n2))


def tukey_hsd(groups: Sequence[Sequence[float]], exact: bool = True) -> list[tuple[int, int, float]]:
    """Tukey HSD pairwise p-values.

    exact=True uses the studentized range distribution (correct).
    exact=False reproduces the Rust approximation, for diffing only.
    """
    k = len(groups)
    means = [mean(g) for g in groups]
    ss_within = math.fsum(
        math.fsum((x - m) ** 2 for x in g) for g, m in zip(groups, means)
    )
    df_within = float(sum(len(g) - 1 for g in groups))
    mse = ss_within / df_within
    out: list[tuple[int, int, float]] = []
    for i, j in itertools.combinations(range(k), 2):
        n_i, n_j = float(len(groups[i])), float(len(groups[j]))
        se = math.sqrt(mse * (1.0 / n_i + 1.0 / n_j) / 2.0)
        if se <= 0.0:
            out.append((i, j, 1.0 if means[i] == means[j] else 0.0))
            continue
        q = abs(means[i] - means[j]) / se
        if exact:
            p = srange_sf(q, k, df_within)
        else:
            # tukey.rs: fold q into a t statistic, then a Bonferroni-like x k.
            p = min(1.0, k * t_sf_two_sided(q / _SQRT2, df_within))
        out.append((i, j, p))
    return out


def tukey_all_pass(groups: Sequence[Sequence[float]], alpha: float) -> bool:
    """Cheap screening: are all Tukey pairwise p-values > alpha?

    Comparing the q statistic against the cached critical value is equivalent
    to comparing p against alpha, and avoids one double integral per pair.
    """
    k = len(groups)
    means = [mean(g) for g in groups]
    ss_within = math.fsum(
        math.fsum((x - m) ** 2 for x in g) for g, m in zip(groups, means)
    )
    df_within = float(sum(len(g) - 1 for g in groups))
    mse = ss_within / df_within
    q_crit = srange_crit(alpha, k, df_within)
    for i, j in itertools.combinations(range(k), 2):
        n_i, n_j = float(len(groups[i])), float(len(groups[j]))
        se = math.sqrt(mse * (1.0 / n_i + 1.0 / n_j) / 2.0)
        if se <= 0.0:
            if means[i] != means[j]:
                return False
            continue
        if abs(means[i] - means[j]) / se >= q_crit:
            return False
    return True


def dunnett_t3(
    groups: Sequence[Sequence[float]], exact: bool = True
) -> list[tuple[int, int, float]]:
    """Dunnett's T3 pairwise p-values (all pairs, not versus a control).

    exact=True adjusts the pairwise Welch statistic through the studentized
    maximum modulus distribution, which is what T3 is defined as.
    exact=False reproduces the Rust behaviour (unadjusted Welch t), for diffing.
    """
    k = len(groups)
    c = k * (k - 1) // 2
    stats = [(mean(g), variance(g), float(len(g))) for g in groups]
    out: list[tuple[int, int, float]] = []
    for i, j in itertools.combinations(range(k), 2):
        m_i, v_i, n_i = stats[i]
        m_j, v_j, n_j = stats[j]
        se = math.sqrt(v_i / n_i + v_j / n_j)
        if se <= 0.0:
            out.append((i, j, 1.0 if m_i == m_j else 0.0))
            continue
        t = abs(m_i - m_j) / se
        df = welch_df(v_i, n_i, v_j, n_j)
        p = smm_sf(t, c, df) if exact else t_sf_two_sided(t, df)
        out.append((i, j, p))
    return out


@dataclass
class IndicatorResult:
    indicator: str
    levene_p: float
    diff_p: float
    method: str
    valid: bool
    posthoc: list[tuple[int, int, float]] | None = None
    # (mean, sd, n) per experimental group - reports almost always need this
    group_stats: list[tuple[float, float, int]] = field(default_factory=list)

    def to_dict(self, alpha: float, group_ids: Sequence[int] | None = None) -> dict:
        """Serialize. `group_ids` maps post-hoc positions back to real group ids,
        which differ from positions whenever a reserve group sits in between."""
        d = {
            "indicator_name": self.indicator,
            "levene_p_value": self.levene_p,
            "diff_p_value": self.diff_p,
            "test_method": self.method,
            "is_valid": self.valid,
            "group_stats": [
                {
                    "group_id": (group_ids[pos] if group_ids else pos),
                    "mean": m,
                    "sd": sd,
                    "n": n,
                }
                for pos, (m, sd, n) in enumerate(self.group_stats)
            ],
        }
        if self.posthoc is not None:
            def gid(pos: int) -> int:
                return group_ids[pos] if group_ids else pos

            d["posthoc_results"] = [
                {
                    "group1_id": gid(i),
                    "group2_id": gid(j),
                    "p_value": p,
                    "is_valid": p > alpha,
                }
                for i, j, p in self.posthoc
            ]
        return d


def test_indicator(
    groups: Sequence[Sequence[float]],
    alpha: float,
    *,
    levene_center: str = "mean",
    exact_posthoc: bool = True,
    screen_only: bool = False,
) -> IndicatorResult:
    """Run the test cascade for one indicator across the experimental groups.

    The cascade is: Levene decides homoscedastic vs heteroscedastic, group
    count decides two-sample vs omnibus, and for >=3 groups every pairwise
    post-hoc comparison must also clear alpha for the indicator to count as
    balanced. `screen_only` skips exact post-hoc p-values and just decides
    pass/fail, which is all the ranking needs.
    """
    k = len(groups)
    p_levene = levene(groups, center=levene_center)
    homoscedastic = p_levene > alpha
    descriptives = [
        (mean(g), math.sqrt(variance(g)), len(g)) for g in groups
    ]

    if k == 2:
        if homoscedastic:
            p = student_ttest(groups[0], groups[1])
            method = "Student t-test"
        else:
            p = welch_ttest(groups[0], groups[1])
            method = "Welch t-test"
        return IndicatorResult(
            indicator="",
            levene_p=p_levene,
            diff_p=p,
            method=method,
            valid=p > alpha,
            group_stats=descriptives,
        )

    if homoscedastic:
        p = one_way_anova(groups)
        method = "One-way ANOVA + Tukey HSD"
        if screen_only:
            posthoc = None
            posthoc_ok = tukey_all_pass(groups, alpha)
        else:
            posthoc = tukey_hsd(groups, exact=exact_posthoc)
            posthoc_ok = all(pp > alpha for _, _, pp in posthoc)
    else:
        p = welch_anova(groups)
        method = "Welch ANOVA + Dunnett's T3"
        posthoc = dunnett_t3(groups, exact=exact_posthoc)
        posthoc_ok = all(pp > alpha for _, _, pp in posthoc)
        if screen_only:
            posthoc = None

    valid = (p > alpha) and posthoc_ok
    return IndicatorResult(
        indicator="",
        levene_p=p_levene,
        diff_p=p,
        method=method,
        valid=valid,
        posthoc=posthoc,
        group_stats=descriptives,
    )


# ---------------------------------------------------------------------------
# Section 3. Data model, enumeration, evaluation
# ---------------------------------------------------------------------------

MALE, FEMALE = "M", "F"
_SEX_ALIASES = {
    "M": MALE, "MALE": MALE, "雄": MALE, "雄性": MALE, "公": MALE,
    "F": FEMALE, "FEMALE": FEMALE, "雌": FEMALE, "雌性": FEMALE, "母": FEMALE,
}


def parse_sex(raw: object) -> str:
    key = str(raw).strip().upper()
    if key not in _SEX_ALIASES:
        raise ValueError(f"invalid sex value: {raw!r}")
    return _SEX_ALIASES[key]


@dataclass
class Animal:
    id: str
    sex: str
    indicators: dict[str, float] = field(default_factory=dict)


@dataclass
class GroupSpec:
    index: int
    male: int
    female: int
    reserve: bool = False
    name: str | None = None

    @property
    def size(self) -> int:
        return self.male + self.female

    @property
    def label(self) -> str:
        return self.name or ("备用动物" if self.reserve else f"G{self.index + 1}")


@dataclass
class StatSpec:
    indicators: list[str]
    alpha: float = 0.05
    mode: str = "strict"  # strict | optimized
    levene_center: str = "mean"
    exact_posthoc: bool = True

    @property
    def max_invalid(self) -> int:
        return 0 if self.mode == "strict" else 1


@dataclass
class CandidateResult:
    groups: list[list[int]]
    stats: list[IndicatorResult]
    min_p: float
    mean_p: float
    num_invalid: int

    @property
    def meets_criteria_strict(self) -> bool:
        return self.num_invalid == 0


def validate_config(animals: Sequence[Animal], specs: Sequence[GroupSpec]) -> None:
    """Reject infeasible configurations before enumerating anything.

    These are the same preconditions the Rust enumerator enforces; failing
    fast here turns "No valid grouping found" into an actionable message.
    """
    males = sum(1 for a in animals if a.sex == MALE)
    females = len(animals) - males
    need_m = sum(s.male for s in specs)
    need_f = sum(s.female for s in specs)
    problems = []
    if need_m + need_f != len(animals):
        problems.append(
            f"group sizes sum to {need_m + need_f} but the dataset has {len(animals)} animals"
        )
    if need_m != males:
        problems.append(f"config needs {need_m} males, dataset has {males}")
    if need_f != females:
        problems.append(f"config needs {need_f} females, dataset has {females}")
    for s in specs:
        if not s.reserve and s.size < 2:
            problems.append(
                f"experimental group {s.index + 1} has {s.size} animal(s); statistics need >= 2"
            )
    if problems:
        raise ValueError("infeasible configuration:\n  - " + "\n  - ".join(problems))


def enumerate_candidates(
    animals: Sequence[Animal],
    specs: Sequence[GroupSpec],
    *,
    limit: int | None = None,
    dedup_symmetric: bool = False,
    seed: int = 20240101,
) -> tuple[list[list[list[int]]], bool]:
    """Enumerate sex-stratified assignments of animals to groups.

    Returns (candidates, sampled). Each candidate is a list of index lists in
    group order. When the exhaustive space exceeds `limit`, fall back to
    seeded random sampling - seeded so a rerun reproduces the same answer,
    which matters when this output is used to audit a Rust run.
    """
    male_idx = [i for i, a in enumerate(animals) if a.sex == MALE]
    female_idx = [i for i, a in enumerate(animals) if a.sex == FEMALE]

    total = 1
    rem_m, rem_f = len(male_idx), len(female_idx)
    for s in specs:
        total *= math.comb(rem_m, s.male) * math.comb(rem_f, s.female)
        rem_m -= s.male
        rem_f -= s.female

    if limit is not None and total > limit:
        rng = random.Random(seed)
        seen: set[tuple] = set()
        out: list[list[list[int]]] = []
        for _ in range(limit * 3):
            if len(out) >= limit:
                break
            m_pool, f_pool = list(male_idx), list(female_idx)
            rng.shuffle(m_pool)
            rng.shuffle(f_pool)
            groups, mo, fo = [], 0, 0
            for s in specs:
                groups.append(m_pool[mo : mo + s.male] + f_pool[fo : fo + s.female])
                mo += s.male
                fo += s.female
            key = tuple(tuple(sorted(g)) for g in groups)
            if key in seen:
                continue
            seen.add(key)
            out.append([sorted(g) for g in groups])
        return out, True

    def walk(
        m_pool: list[int], f_pool: list[int], rest: Sequence[GroupSpec], acc: list[list[int]]
    ) -> Iterable[list[list[int]]]:
        if len(rest) == 1:
            if len(m_pool) == rest[0].male and len(f_pool) == rest[0].female:
                yield acc + [list(m_pool) + list(f_pool)]
            return
        head = rest[0]
        for m_combo in itertools.combinations(m_pool, head.male):
            m_left = [i for i in m_pool if i not in m_combo]
            for f_combo in itertools.combinations(f_pool, head.female):
                f_left = [i for i in f_pool if i not in f_combo]
                yield from walk(
                    m_left, f_left, rest[1:], acc + [list(m_combo) + list(f_combo)]
                )

    candidates = list(walk(male_idx, female_idx, specs, []))
    if dedup_symmetric:
        candidates = _dedup_symmetric(candidates, specs)
    return candidates, False


def _dedup_symmetric(
    candidates: list[list[list[int]]], specs: Sequence[GroupSpec]
) -> list[list[list[int]]]:
    """Drop candidates that are relabelings of one already kept.

    Groups with identical quotas and identical roles are interchangeable: every
    statistic is symmetric in them, so permuting them yields the same balance
    with different group numbers. Keeping one representative stops the Top-N
    list from filling up with the same partition under different names.
    """
    classes: dict[tuple, list[int]] = {}
    for s in specs:
        classes.setdefault((s.male, s.female, s.reserve, s.name), []).append(s.index)
    kept = []
    for cand in candidates:
        canonical = True
        for members in classes.values():
            if len(members) < 2:
                continue
            mins = [min(cand[i]) for i in members]
            if mins != sorted(mins):
                canonical = False
                break
        if canonical:
            kept.append(cand)
    return kept


def _group_values(
    animals: Sequence[Animal],
    groups: Sequence[Sequence[int]],
    specs: Sequence[GroupSpec],
    indicator: str,
) -> list[list[float]] | None:
    """Collect one indicator's values per experimental group.

    Reserve groups are excluded - they are spare animals, not a comparison
    arm. Returns None when any experimental group has fewer than two usable
    values, in which case the indicator cannot be tested.
    """
    out = []
    for spec, members in zip(specs, groups):
        if spec.reserve:
            continue
        vals = [
            animals[i].indicators[indicator]
            for i in members
            if indicator in animals[i].indicators
        ]
        if len(vals) < 2:
            return None
        out.append(vals)
    return out if len(out) >= 2 else None


def evaluate_candidate(
    animals: Sequence[Animal],
    groups: Sequence[Sequence[int]],
    specs: Sequence[GroupSpec],
    stat: StatSpec,
    *,
    screen_only: bool = False,
) -> CandidateResult:
    stats: list[IndicatorResult] = []
    min_p, sum_p, invalid = float("inf"), 0.0, 0
    for name in stat.indicators:
        groups_vals = _group_values(animals, groups, specs, name)
        if groups_vals is None:
            continue
        res = test_indicator(
            groups_vals,
            stat.alpha,
            levene_center=stat.levene_center,
            exact_posthoc=stat.exact_posthoc,
            screen_only=screen_only,
        )
        res.indicator = name
        if not res.valid:
            invalid += 1
        min_p = min(min_p, res.diff_p)
        sum_p += res.diff_p
        stats.append(res)
    mean_p = sum_p / len(stats) if stats else float("nan")
    if not stats:
        min_p = float("nan")
    return CandidateResult(
        groups=[list(g) for g in groups],
        stats=stats,
        min_p=min_p,
        mean_p=mean_p,
        num_invalid=invalid,
    )


def rank_candidates(
    animals: Sequence[Animal],
    candidates: Sequence[Sequence[Sequence[int]]],
    specs: Sequence[GroupSpec],
    stat: StatSpec,
    top_n: int = 10,
) -> tuple[list[CandidateResult], int]:
    """Screen every candidate, then re-score the survivors with exact post-hoc.

    Ranking only ever reads the omnibus p-value (min then mean), so the
    screening pass can skip exact post-hoc p-values entirely and still produce
    the same ordering and the same valid/invalid verdicts.
    """
    scored = []
    for cand in candidates:
        res = evaluate_candidate(animals, cand, specs, stat, screen_only=True)
        if res.num_invalid <= stat.max_invalid:
            scored.append(res)
    total_valid = len(scored)
    scored.sort(key=lambda r: (-r.min_p, -r.mean_p))
    best = scored[:top_n]
    detailed = [
        evaluate_candidate(animals, r.groups, specs, stat, screen_only=False) for r in best
    ]
    return detailed, total_valid


# ---------------------------------------------------------------------------
# Section 4. Input / output
# ---------------------------------------------------------------------------


def _col_index(ref: str) -> int:
    letters = "".join(ch for ch in ref if ch.isalpha())
    idx = 0
    for ch in letters:
        idx = idx * 26 + (ord(ch.upper()) - 64)
    return idx - 1


def read_xlsx(path: str, sheet: int = 0) -> list[list[object]]:
    """Minimal .xlsx reader (values only), so no third-party package is needed."""
    ns = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
    with zipfile.ZipFile(path) as zf:
        shared: list[str] = []
        if "xl/sharedStrings.xml" in zf.namelist():
            root = ET.fromstring(zf.read("xl/sharedStrings.xml"))
            for si in root.findall(f"{ns}si"):
                shared.append("".join(t.text or "" for t in si.iter(f"{ns}t")))
        sheets = sorted(
            n for n in zf.namelist() if re.fullmatch(r"xl/worksheets/sheet\d+\.xml", n)
        )
        if not sheets:
            raise ValueError(f"{path}: no worksheets found")
        root = ET.fromstring(zf.read(sheets[min(sheet, len(sheets) - 1)]))

    rows: list[list[object]] = []
    for row_el in root.iter(f"{ns}row"):
        row: list[object] = []
        for cell in row_el.findall(f"{ns}c"):
            ref = cell.get("r")
            if ref:
                target = _col_index(ref)
                while len(row) < target:
                    row.append(None)
            ctype = cell.get("t")
            value: object = None
            if ctype == "inlineStr":
                is_el = cell.find(f"{ns}is")
                value = "".join(t.text or "" for t in is_el.iter(f"{ns}t")) if is_el is not None else None
            else:
                v = cell.find(f"{ns}v")
                raw = v.text if v is not None else None
                if raw is None:
                    value = None
                elif ctype == "s":
                    value = shared[int(raw)]
                elif ctype in ("str", "e"):
                    value = raw
                elif ctype == "b":
                    value = raw == "1"
                else:
                    try:
                        value = float(raw)
                    except ValueError:
                        value = raw
            row.append(value)
        rows.append(row)
    return rows


_HEADER_KEYWORDS = ("动物编号", "编号", "性别", "animalid", "animal", "sex", "id")
_PURE_UNITS = {
    "kg", "g", "mg", "ug", "ng", "℃", "°C", "L", "mL", "uL", "U", "IU", "mol",
    "mmol", "umol", "nmol", "m", "cm", "mm", "sec", "min", "h", "%", "fL", "pg",
    "U/L", "g/L", "mg/L", "mmol/L", "umol/L", "nmol/L", "10^9/L", "10^12/L",
    "deg", "A/G", "AST/ALT",
}
_SIMPLE_NAMES = {"kg", "℃", "°C", "cm", "g"}


def _has_chinese(s: str) -> bool:
    return any("一" <= ch <= "鿿" for ch in s)


def _is_unit(s: str) -> bool:
    if not s:
        return False
    if s in _PURE_UNITS:
        return True
    if "(" in s:
        head = s[: s.index("(")]
        if head and any(ch.isupper() for ch in head):
            return False
    return len(s) <= 10 and ("/" in s or "^" in s or "mol" in s)


def _header_key(prev: str, curr: str) -> str:
    """Reproduce parser.rs::parse_dual_row_header key selection.

    Indicator keys must match the Rust parser exactly, otherwise
    `--indicators kg,ALT` would silently select nothing.
    """
    # Case order matters and follows the Rust function exactly: a simple name
    # above a Chinese name wins before the unit heuristics get a say, which is
    # why "kg" / "℃" become keys rather than being treated as bare units.
    if prev in _SIMPLE_NAMES and _has_chinese(curr) and not _is_unit(curr):
        return prev
    if prev and not _is_unit(prev) and prev not in _SIMPLE_NAMES and _is_unit(curr):
        return prev
    if _is_unit(prev) and not _is_unit(curr):
        return curr
    if _has_chinese(curr):
        return curr
    if prev:
        return prev
    return curr


def dataset_from_xlsx(path: str, sheet: int = 0) -> tuple[list[Animal], list[str]]:
    rows = read_xlsx(path, sheet)
    header_idx = None
    for i, row in enumerate(rows):
        head = " ".join(str(c).lower() for c in row[:2] if c is not None)
        if any(kw.lower() in head for kw in _HEADER_KEYWORDS):
            header_idx = i
            break
    if header_idx is None:
        header_idx = 1
    header = rows[header_idx]
    prev = rows[header_idx - 1] if header_idx > 0 else []

    keys: list[str] = []
    col_of: list[int] = []
    for col in range(2, len(header)):
        curr = str(header[col]).strip() if header[col] is not None else ""
        above = str(prev[col]).strip() if col < len(prev) and prev[col] is not None else ""
        if not curr:
            continue
        keys.append(_header_key(above, curr))
        col_of.append(col)

    animals: list[Animal] = []
    for row in rows[header_idx + 1 :]:
        if not row or row[0] is None or (isinstance(row[0], str) and not row[0].strip()):
            continue
        raw_id = row[0]
        aid = (
            f"{raw_id:.0f}"
            if isinstance(raw_id, float) and raw_id.is_integer()
            else str(raw_id).strip()
        )
        if len(row) < 2 or row[1] is None:
            continue
        sex = parse_sex(row[1])
        indicators = {}
        for key, col in zip(keys, col_of):
            if col < len(row) and isinstance(row[col], (int, float)) and not isinstance(row[col], bool):
                indicators[key] = float(row[col])
        animals.append(Animal(id=aid, sex=sex, indicators=indicators))
    return animals, keys


def animals_from_json(payload: dict) -> list[Animal]:
    out = []
    for raw in payload["animals"]:
        out.append(
            Animal(
                id=str(raw["id"]),
                sex=parse_sex(raw["sex"]),
                indicators={k: float(v) for k, v in (raw.get("indicators") or {}).items()},
            )
        )
    return out


def parse_group_spec(text: str) -> list[GroupSpec]:
    """Parse "3M+2F,3M+1F,2M+0F:reserve" into group specs.

    Also accepts a plain size list ("5,4") when the dataset is single-sex, and
    ":reserve" / ":reserve=名称" suffixes for spare-animal groups.
    """
    specs: list[GroupSpec] = []
    for idx, chunk in enumerate(text.split(",")):
        chunk = chunk.strip()
        if not chunk:
            continue
        reserve = False
        name = None
        if ":" in chunk:
            chunk, tag = chunk.split(":", 1)
            tag = tag.strip()
            if tag.startswith("reserve"):
                reserve = True
                if "=" in tag:
                    name = tag.split("=", 1)[1].strip() or None
            else:
                name = tag or None
            chunk = chunk.strip()
        m = re.fullmatch(r"(?:(\d+)\s*[Mm])?\s*(?:\+?\s*(\d+)\s*[Ff])?", chunk)
        if m and (m.group(1) or m.group(2)):
            male = int(m.group(1) or 0)
            female = int(m.group(2) or 0)
        elif chunk.isdigit():
            male, female = int(chunk), 0
        else:
            raise ValueError(f"cannot parse group spec {chunk!r}; use forms like '3M+2F'")
        specs.append(GroupSpec(index=idx, male=male, female=female, reserve=reserve, name=name))
    if not specs:
        raise ValueError("empty group specification")
    return specs


def experimental_labels(specs: Sequence[GroupSpec]) -> list[str]:
    """Labels of the groups that actually enter the statistics, in order.

    Post-hoc comparisons are indexed by position among experimental groups, so
    a reserve group in the middle would otherwise shift every label.
    """
    return [s.label for s in specs if not s.reserve]


def format_candidate(
    animals: Sequence[Animal],
    res: CandidateResult,
    specs: Sequence[GroupSpec],
    stat: StatSpec,
    rank: int | None = None,
    show_means: bool = False,
) -> str:
    exp_labels = experimental_labels(specs)
    lines = []
    head = "候选方案" if rank is None else f"候选方案 #{rank}"
    lines.append(f"{head}: min(P)={res.min_p:.6f}  mean(P)={res.mean_p:.6f}  "
                 f"未通过指标={res.num_invalid}/{len(res.stats)}")
    for spec, members in zip(specs, res.groups):
        tag = " [备用/不参与统计]" if spec.reserve else ""
        ids = ", ".join(
            f"{animals[i].id}({'雄' if animals[i].sex == MALE else '雌'})" for i in members
        )
        lines.append(f"  {spec.label}{tag}: {ids}")
    lines.append(f"  {'指标':<14}{'Levene P':>12}{'差异 P':>12}  {'判定':<6}检验方法")
    for s in res.stats:
        verdict = "通过" if s.valid else "未通过"
        lines.append(
            f"  {s.indicator:<14}{s.levene_p:>12.6f}{s.diff_p:>12.6f}  {verdict:<6}{s.method}"
        )
        if show_means and s.group_stats:
            desc = "  ".join(
                f"{exp_labels[pos]}: {m:.4g}±{sd:.3g} (n={n})"
                for pos, (m, sd, n) in enumerate(s.group_stats)
            )
            lines.append(f"      {desc}")
        if s.posthoc:
            for i, j, p in s.posthoc:
                mark = "ok" if p > stat.alpha else "FAIL"
                lines.append(
                    f"      事后比较 {exp_labels[i]} vs {exp_labels[j]}: P={p:.6f} [{mark}]"
                )
    return "\n".join(lines)


def candidate_to_dict(
    animals: Sequence[Animal],
    res: CandidateResult,
    specs: Sequence[GroupSpec],
    alpha: float,
) -> dict:
    exp_ids = [s.index for s in specs if not s.reserve]
    return {
        "assignments": [
            {
                "animal_id": animals[i].id,
                "sex": "Male" if animals[i].sex == MALE else "Female",
                "group_id": specs[pos].index,
                "group_label": specs[pos].label,
                "is_reserve": specs[pos].reserve,
            }
            for pos, members in enumerate(res.groups)
            for i in members
        ],
        "statistics": [s.to_dict(alpha, exp_ids) for s in res.stats],
        "summary": {
            "min_p_value": res.min_p,
            "mean_p_value": res.mean_p,
            "num_invalid_indicators": res.num_invalid,
            "total_indicators": len(res.stats),
            "passed_indicators": len(res.stats) - res.num_invalid,
        },
    }


# ---------------------------------------------------------------------------
# Section 5. CLI
# ---------------------------------------------------------------------------


def _load_dataset(args) -> tuple[list[Animal], list[str]]:
    if args.excel:
        return dataset_from_xlsx(args.excel, args.sheet)
    payload = json.loads(open(args.input, encoding="utf-8").read())
    animals = animals_from_json(payload)
    keys: list[str] = []
    for a in animals:
        for k in a.indicators:
            if k not in keys:
                keys.append(k)
    return animals, keys


def _resolve_indicators(args, available: Sequence[str]) -> list[str]:
    if args.indicators in (None, "all"):
        return list(available)
    wanted = [s.strip() for s in args.indicators.split(",") if s.strip()]
    missing = [w for w in wanted if w not in available]
    if missing:
        raise SystemExit(
            f"未找到指标 {missing}. 可用指标 ({len(available)}): {', '.join(available)}"
        )
    return wanted


def warn_skipped(requested: Sequence[str], res: CandidateResult) -> None:
    """Say out loud which indicators never made it into the statistics.

    An indicator with fewer than two usable values per group is dropped
    silently by both engines, which makes "all indicators passed" read as
    stronger than it is. Naming the dropped ones keeps the claim honest.
    """
    tested = {s.indicator for s in res.stats}
    skipped = [name for name in requested if name not in tested]
    if skipped:
        print(
            f"注意: {len(skipped)} 个指标未参与检验（每组有效数值不足 2 个）: "
            f"{', '.join(skipped)}\n"
            f"      实际检验 {len(res.stats)} 个 / 所选 {len(requested)} 个，"
            f"报告通过率时要写清这一点。"
        )


def _stat_spec(args, indicators: Sequence[str]) -> StatSpec:
    return StatSpec(
        indicators=list(indicators),
        alpha=args.alpha,
        mode=args.mode,
        levene_center=args.levene,
        exact_posthoc=(args.posthoc == "exact"),
    )


def cmd_group(args) -> int:
    animals, available = _load_dataset(args)
    specs = parse_group_spec(args.groups)
    validate_config(animals, specs)
    stat = _stat_spec(args, _resolve_indicators(args, available))

    candidates, sampled = enumerate_candidates(
        animals, specs, limit=args.max_enumerate, dedup_symmetric=args.dedup, seed=args.seed
    )
    results, total_valid = rank_candidates(animals, candidates, specs, stat, args.top)

    report = {
        "engine": "python-exact",
        "alpha": stat.alpha,
        "mode": stat.mode,
        "levene_center": stat.levene_center,
        "posthoc": "exact" if stat.exact_posthoc else "rust-approx",
        "total_animals": len(animals),
        "total_evaluated": len(candidates),
        "total_valid": total_valid,
        "sampled": sampled,
        "dedup_symmetric": args.dedup,
        "candidates": [candidate_to_dict(animals, r, specs, stat.alpha) for r in results],
    }
    if args.output:
        with open(args.output, "w", encoding="utf-8") as fh:
            json.dump(report, fh, ensure_ascii=False, indent=2)

    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
        return 0 if results else 1

    print(
        f"动物 {len(animals)} 只, 指标 {len(stat.indicators)} 个, "
        f"候选 {len(candidates)}{' (随机抽样)' if sampled else ''}, "
        f"满足条件 {total_valid}, 模式 {stat.mode}, α={stat.alpha}"
    )
    if not results:
        print("\n没有满足条件的分组方案。可尝试: 放宽为 --mode optimized、减少指标、"
              "或检查是否有指标本身在个体间差异极大。")
        return 1
    warn_skipped(stat.indicators, results[0])
    for rank, res in enumerate(results, start=1):
        print()
        print(format_candidate(animals, res, specs, stat, rank, show_means=args.means))
    if args.output:
        print(f"\n完整结果已写入 {args.output}")
    return 0


def extract_assignments(payload: object, candidate: int = 0) -> list[dict]:
    """Pull an assignment list out of whatever JSON shape the caller has.

    Accepted shapes, so that both this tool's own `group --output` and the Rust
    engine's result JSON can be fed straight back in:
      [ {...} ]                                  bare list
      {"assignments": [...]}                     single result
      {"candidates": [{"assignments": [...]}]}   Top-N result, pick by index
    """
    if isinstance(payload, list):
        return payload
    if isinstance(payload, dict):
        if isinstance(payload.get("assignments"), list):
            return payload["assignments"]
        cands = payload.get("candidates")
        if isinstance(cands, list) and cands:
            if candidate >= len(cands):
                raise ValueError(
                    f"--candidate {candidate} 超出范围，文件里只有 {len(cands)} 个候选"
                )
            inner = cands[candidate]
            if isinstance(inner, dict) and isinstance(inner.get("assignments"), list):
                return inner["assignments"]
        if isinstance(payload.get("assignments"), dict):
            return extract_assignments(payload["assignments"], candidate)
    raise ValueError(
        "分配文件里找不到 assignments。支持的结构: 顶层数组、{\"assignments\": [...]}、"
        "或 {\"candidates\": [{\"assignments\": [...]}]}（用 --candidate 选第几个）"
    )


def cmd_verify(args) -> int:
    animals, available = _load_dataset(args)
    by_id = {a.id: i for i, a in enumerate(animals)}

    payload = json.loads(open(args.assignments, encoding="utf-8").read())
    raw = extract_assignments(payload, args.candidate)
    groups_map: dict[int, list[int]] = {}
    flagged_reserve: set[int] = set()
    for item in raw:
        aid = str(item["animal_id"])
        if aid not in by_id:
            raise SystemExit(f"分配中的动物 {aid!r} 不在数据集中")
        gid = int(item["group_id"])
        groups_map.setdefault(gid, []).append(by_id[aid])
        if item.get("is_reserve"):
            flagged_reserve.add(gid)

    reserve = {int(x) for x in (args.reserve or "").replace(" ", "").split(",") if x}
    # Records that already say which groups are reserve save the caller from
    # repeating it; an explicit --reserve still wins.
    if not reserve and flagged_reserve:
        reserve = flagged_reserve
        print(f"（按分配文件标记，视为备用组: {sorted(reserve)}）")
    order = sorted(groups_map)
    groups = [groups_map[g] for g in order]
    specs = [
        GroupSpec(
            index=g,  # keep the caller's group_id so labels stay traceable
            male=sum(1 for i in groups_map[g] if animals[i].sex == MALE),
            female=sum(1 for i in groups_map[g] if animals[i].sex == FEMALE),
            reserve=g in reserve,
        )
        for g in order
    ]
    stat = _stat_spec(args, _resolve_indicators(args, available))

    res = evaluate_candidate(animals, groups, specs, stat, screen_only=False)
    warn_skipped(stat.indicators, res)
    print(format_candidate(animals, res, specs, stat, show_means=args.means))
    verdict = res.num_invalid <= stat.max_invalid
    print(f"\n判定 ({stat.mode}): {'满足' if verdict else '不满足'}标准 "
          f"(未通过指标 {res.num_invalid}, 允许 {stat.max_invalid})")

    if args.compare:
        rust = StatSpec(
            indicators=stat.indicators,
            alpha=stat.alpha,
            mode=stat.mode,
            levene_center=stat.levene_center,
            exact_posthoc=False,
        )
        approx = evaluate_candidate(animals, groups, specs, rust, screen_only=False)
        print("\n精确事后检验 vs Rust 近似 (仅列出有差异的指标):")
        diffs = 0
        for a, b in zip(res.stats, approx.stats):
            if a.posthoc is None or b.posthoc is None:
                continue
            for (i, j, pa), (_, _, pb) in zip(a.posthoc, b.posthoc):
                if abs(pa - pb) > 1e-6 or (pa > stat.alpha) != (pb > stat.alpha):
                    flag = " <-- 判定不同" if (pa > stat.alpha) != (pb > stat.alpha) else ""
                    print(
                        f"  {a.indicator:<12} G{i + 1} vs G{j + 1}: "
                        f"精确 P={pa:.6f}  Rust P={pb:.6f}{flag}"
                    )
                    diffs += 1
        if diffs == 0:
            print("  无差异")
    if args.output:
        with open(args.output, "w", encoding="utf-8") as fh:
            json.dump(
                candidate_to_dict(animals, res, specs, stat.alpha),
                fh,
                ensure_ascii=False,
                indent=2,
            )
        print(f"结果已写入 {args.output}")
    return 0 if verdict else 1


# --- self test --------------------------------------------------------------


def _mc_srange_sf(q: float, k: int, nu: float, n: int, seed: int) -> float:
    rng = random.Random(seed)
    hits = 0
    for _ in range(n):
        xs = [rng.gauss(0.0, 1.0) for _ in range(k)]
        chi = math.fsum(rng.gauss(0.0, 1.0) ** 2 for _ in range(int(nu)))
        s = math.sqrt(chi / nu)
        if (max(xs) - min(xs)) / s > q:
            hits += 1
    return hits / n


def _mc_smm_sf(m: float, c: int, nu: float, n: int, seed: int) -> float:
    rng = random.Random(seed)
    hits = 0
    for _ in range(n):
        xs = [abs(rng.gauss(0.0, 1.0)) for _ in range(c)]
        chi = math.fsum(rng.gauss(0.0, 1.0) ** 2 for _ in range(int(nu)))
        s = math.sqrt(chi / nu)
        if max(xs) / s > m:
            hits += 1
    return hits / n


def cmd_self_test(args) -> int:
    checks: list[tuple[str, bool, str]] = []

    def check(name: str, ok: bool, detail: str = "") -> None:
        checks.append((name, ok, detail))

    # betainc against direct numeric integration of the beta density. The
    # substitution t = u**(1/a) removes the integrable singularity at t=0, so
    # plain quadrature stays accurate even for a < 1.
    for a, b, x in ((2.5, 3.5, 0.3), (0.5, 8.0, 0.7), (11.0, 4.0, 0.42)):
        log_norm = math.lgamma(a + b) - math.lgamma(a) - math.lgamma(b)
        scale = math.exp(log_norm) / a
        num = _integrate(
            lambda u: scale * (1.0 - u ** (1.0 / a)) ** (b - 1.0),
            0.0,
            x**a,
            panels=160,
            nodes=16,
        )
        got = betainc(a, b, x)
        check(f"betainc({a},{b},{x}) vs quadrature", abs(got - num) < 1e-6, f"{got:.12f} vs {num:.12f}")

    # Student t tail against integration of the t density.
    for t, df in ((0.5, 8.0), (2.306, 8.0), (3.1, 15.0)):
        log_c = math.lgamma((df + 1) / 2) - math.lgamma(df / 2) - 0.5 * math.log(df * math.pi)
        tail = _integrate(
            lambda x: math.exp(log_c - (df + 1) / 2 * math.log1p(x * x / df)), t, t + 60.0,
            panels=200, nodes=16,
        )
        got = t_sf_two_sided(t, df)
        check(f"t_sf(t={t}, df={df}) vs quadrature", abs(got - 2 * tail) < 1e-8,
              f"{got:.10f} vs {2 * tail:.10f}")

    # Two-tailed t at the classic table point: t(0.975, 8) = 2.306.
    check("t table point t(0.975,8)=2.306", abs(t_sf_two_sided(2.306, 8.0) - 0.05) < 5e-4,
          f"p={t_sf_two_sided(2.306, 8.0):.6f}")

    # ANOVA with two groups must equal Student's t-test.
    g = [[1.0, 2.0, 3.0, 4.0, 5.0], [1.5, 2.6, 3.4, 4.9, 5.2]]
    check("ANOVA(k=2) == Student t", abs(one_way_anova(g) - student_ttest(g[0], g[1])) < 1e-12,
          f"{one_way_anova(g):.12f} vs {student_ttest(g[0], g[1]):.12f}")

    # Welch ANOVA with two groups must equal Welch's t-test.
    check("Welch ANOVA(k=2) == Welch t", abs(welch_anova(g) - welch_ttest(g[0], g[1])) < 1e-9,
          f"{welch_anova(g):.12f} vs {welch_ttest(g[0], g[1]):.12f}")

    # Studentized range with k=2 reduces exactly to the two-tailed t.
    for q, nu in ((1.5, 10.0), (3.15, 10.0), (2.0, 24.0)):
        exact = t_sf_two_sided(q / _SQRT2, nu)
        got = srange_sf(q, 2, nu)
        check(f"srange(k=2, q={q}, nu={nu}) == t two-sided", abs(got - exact) < 1e-6,
              f"{got:.8f} vs {exact:.8f}")

    # Studentized maximum modulus with one comparison reduces to the t as well.
    for m, nu in ((2.0, 12.0), (3.0, 20.0)):
        exact = t_sf_two_sided(m, nu)
        got = smm_sf(m, 1, nu)
        check(f"smm(c=1, m={m}, nu={nu}) == t two-sided", abs(got - exact) < 1e-6,
              f"{got:.8f} vs {exact:.8f}")

    # Published Tukey critical values (alpha=0.05).
    for k, nu, expected in ((3, 12.0, 3.773), (4, 20.0, 3.958), (5, 10.0, 4.654)):
        got = srange_crit(0.05, k, nu)
        check(f"Tukey q(0.05, k={k}, nu={int(nu)}) = {expected}", abs(got - expected) < 5e-3,
              f"{got:.4f}")

    # Monte-Carlo cross-check of both post-hoc distributions.
    n = args.mc_samples
    for q, k, nu in ((3.0, 3, 12.0), (4.0, 4, 15.0)):
        mc = _mc_srange_sf(q, k, nu, n, seed=7)
        got = srange_sf(q, k, nu)
        tol = 4.0 * math.sqrt(max(mc, 1e-6) * (1 - mc) / n) + 0.002
        check(f"srange MC q={q}, k={k}, nu={int(nu)}", abs(got - mc) < tol,
              f"exact={got:.5f} mc={mc:.5f} tol={tol:.5f}")
    for m, c, nu in ((2.5, 3, 12.0), (3.0, 6, 20.0)):
        mc = _mc_smm_sf(m, c, nu, n, seed=11)
        got = smm_sf(m, c, nu)
        tol = 4.0 * math.sqrt(max(mc, 1e-6) * (1 - mc) / n) + 0.002
        check(f"smm MC m={m}, c={c}, nu={int(nu)}", abs(got - mc) < tol,
              f"exact={got:.5f} mc={mc:.5f} tol={tol:.5f}")

    # Levene on identical spreads should be far from significant, and the
    # mean/median variants should agree on symmetric data.
    same = [[1.0, 2.0, 3.0, 4.0, 5.0], [2.0, 3.0, 4.0, 5.0, 6.0], [0.0, 1.0, 2.0, 3.0, 4.0]]
    check("Levene(identical spreads) == 1", abs(levene(same) - 1.0) < 1e-9, f"{levene(same):.6f}")
    check("Levene mean vs median on symmetric data",
          abs(levene(same, "mean") - levene(same, "median")) < 1e-9)

    # Enumeration counts must match the combinatorics the Rust tests assert.
    animals = [Animal(f"M{i}", MALE) for i in range(6)] + [Animal(f"F{i}", FEMALE) for i in range(4)]
    specs = parse_group_spec("3M+2F,3M+2F")
    cands, _ = enumerate_candidates(animals, specs)
    check("enumerate 2 groups (6M/4F, 3M+2F each) == 120", len(cands) == 120, f"{len(cands)}")
    check("dedup halves the symmetric 2-group space",
          len(_dedup_symmetric(cands, specs)) == 60, f"{len(_dedup_symmetric(cands, specs))}")
    animals3 = [Animal(f"M{i}", MALE) for i in range(6)] + [Animal(f"F{i}", FEMALE) for i in range(3)]
    cands3, _ = enumerate_candidates(animals3, parse_group_spec("2M+1F,2M+1F,2M+1F"))
    check("enumerate 3 groups (6M/3F, 2M+1F each) == 540", len(cands3) == 540, f"{len(cands3)}")

    # Every candidate must be a partition: no duplicates, nothing dropped.
    ok = all(
        sorted(i for g in c for i in g) == list(range(9)) for c in cands3
    )
    check("candidates are exact partitions", ok)

    width = max(len(name) for name, _, _ in checks)
    failed = 0
    for name, ok, detail in checks:
        status = "PASS" if ok else "FAIL"
        if not ok:
            failed += 1
        suffix = f"  {detail}" if detail and (args.verbose or not ok) else ""
        print(f"[{status}] {name:<{width}}{suffix}")
    print(f"\n{len(checks) - failed}/{len(checks)} checks passed")
    return 1 if failed else 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="AutoGroup reference grouping engine (exact statistics, stdlib only)"
    )
    sub = parser.add_subparsers(dest="command", required=True)

    def add_data_args(p: argparse.ArgumentParser) -> None:
        src = p.add_mutually_exclusive_group(required=True)
        src.add_argument("--excel", help="path to the source .xlsx (dual-row header supported)")
        src.add_argument("--input", help="path to a JSON dataset {\"animals\": [...]}")
        p.add_argument("--sheet", type=int, default=0, help="worksheet index for --excel (default 0)")
        p.add_argument("--indicators", default="all",
                       help="comma-separated indicator keys, or 'all' (default)")
        p.add_argument("--alpha", type=float, default=0.05)
        p.add_argument("--mode", choices=("strict", "optimized"), default="strict",
                       help="strict: every indicator must pass; optimized: at most one may fail")
        p.add_argument("--levene", choices=("mean", "median"), default="mean",
                       help="mean = Rust parity; median = Brown-Forsythe (more robust)")
        p.add_argument("--posthoc", choices=("exact", "rust"), default="exact",
                       help="exact = studentized range / maximum modulus; rust = reproduce approximation")
        p.add_argument("--means", action="store_true",
                       help="也打印每组的 mean±SD（写报告用；JSON 输出始终包含）")
        p.add_argument("--output", help="write the result as JSON to this path")

    p_group = sub.add_parser("group", help="enumerate and rank candidate groupings")
    add_data_args(p_group)
    p_group.add_argument("--groups", required=True,
                         help="group quotas, e.g. '3M+2F,3M+1F' or '4M+2F,4M+2F,2M+1F:reserve'")
    p_group.add_argument("--top", type=int, default=5, help="how many candidates to report")
    p_group.add_argument("--dedup", action="store_true",
                         help="drop relabelings of an identical partition")
    p_group.add_argument("--max-enumerate", type=int, default=200000,
                         help="switch to seeded random sampling beyond this many candidates")
    p_group.add_argument("--seed", type=int, default=20240101, help="sampling seed")
    p_group.add_argument("--json", action="store_true", help="print JSON instead of a table")
    p_group.set_defaults(func=cmd_group)

    p_verify = sub.add_parser("verify", help="recompute exact statistics for a fixed assignment")
    add_data_args(p_verify)
    p_verify.add_argument("--assignments", required=True,
                          help="JSON 分配文件：顶层数组、{assignments:[...]}，或本工具 group --output "
                               "写出的 {candidates:[...]}（配合 --candidate）")
    p_verify.add_argument("--candidate", type=int, default=0,
                          help="当分配文件含多个候选时，复核第几个（默认 0，即最优）")
    p_verify.add_argument("--reserve", default="",
                          help="comma-separated group_ids to exclude from statistics")
    p_verify.add_argument("--compare", action="store_true",
                          help="also show the Rust post-hoc approximation side by side")
    p_verify.set_defaults(func=cmd_verify)

    p_test = sub.add_parser("self-test", help="validate the statistical kernels")
    p_test.add_argument("--mc-samples", type=int, default=40000,
                        help="Monte-Carlo sample size for the cross-checks")
    p_test.add_argument("--verbose", action="store_true")
    p_test.set_defaults(func=cmd_self_test)

    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        return args.func(args)
    except (ValueError, KeyError) as exc:
        print(f"错误: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
