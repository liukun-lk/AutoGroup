"""Estimate the acceptance rate of a purely random allocation, i.e. the probability
that a random split passes the full balance cascade (Levene -> ANOVA/Welch -> post-hoc).

This is the number that decides whether constrained randomization is cheap or expensive,
and whether plain randomization is actually risky.
"""

import math
import random
import statistics
import sys

sys.path.insert(0, ".claude/skills/animal-grouping/scripts")
from grouping_engine import test_indicator  # noqa: E402

ALPHA = 0.05
N_TRIALS = 2000


def split(values_per_indicator, order, sizes):
    """Split each indicator's values into groups following `order`."""
    out = []
    for values in values_per_indicator:
        groups, pos = [], 0
        for size in sizes:
            groups.append([values[i] for i in order[pos : pos + size]])
            pos += size
        out.append(groups)
    return out


def acceptance_rate(values_per_indicator, sizes, trials=N_TRIALS, seed=12345):
    rng = random.Random(seed)
    n = sum(sizes)
    order = list(range(n))
    accepted = strict = 0
    min_ps = []
    for _ in range(trials):
        rng.shuffle(order)
        n_invalid = 0
        min_p = 1.0
        for groups in split(values_per_indicator, order, sizes):
            res = test_indicator(groups, ALPHA, screen_only=True)
            if not res.valid:
                n_invalid += 1
            min_p = min(min_p, res.diff_p)
        min_ps.append(min_p)
        if n_invalid == 0:
            strict += 1
        if n_invalid <= 1:
            accepted += 1
    return strict / trials, accepted / trials, statistics.median(min_ps)


def synth(rng, kind, n):
    if kind == "bw_normal":  # BW 18-24 g, tight
        return [rng.gauss(21.0, 1.4) for _ in range(n)]
    if kind == "cd45_uniform":  # CD45% 7.4-63, flat and wide
        return [rng.uniform(7.4, 63.0) for _ in range(n)]
    if kind == "cd45_lognormal":  # same span but strongly right-skewed
        return [math.exp(rng.gauss(math.log(18.0), 0.75)) for _ in range(n)]
    if kind == "cd45_bimodal":  # two subpopulations - worst realistic case
        return [
            rng.gauss(12.0, 3.0) if rng.random() < 0.6 else rng.gauss(52.0, 6.0)
            for _ in range(n)
        ]
    raise ValueError(kind)


def main():
    rng = random.Random(7)
    sizes = [12, 12, 12]

    print(f"=== 36 animals -> 3 x 12, alpha={ALPHA}, {N_TRIALS} random splits each ===")
    print(f"{'scenario':<34}{'strict pass':>12}{'<=1 invalid':>13}{'median min(P)':>15}")
    for kind in ("cd45_uniform", "cd45_lognormal", "cd45_bimodal"):
        bw = synth(rng, "bw_normal", 36)
        cd = synth(rng, kind, 36)
        s, o, m = acceptance_rate([bw, cd], sizes)
        print(f"{'BW(normal) + ' + kind:<34}{s:>12.1%}{o:>13.1%}{m:>15.3f}")

    # single indicator, for reference: pure type-I error rate of the cascade
    for kind in ("bw_normal", "cd45_bimodal"):
        v = synth(rng, kind, 36)
        s, o, m = acceptance_rate([v], sizes)
        print(f"{kind + ' alone':<34}{s:>12.1%}{o:>13.1%}{m:>15.3f}")

    # How the rate decays with the number of indicators (independent normals).
    print()
    print("=== acceptance rate vs number of indicators (independent normal data) ===")
    for k in (1, 2, 5, 10, 20, 40, 70):
        vals = [synth(rng, "bw_normal", 36) for _ in range(k)]
        s, o, _ = acceptance_rate(vals, sizes, trials=400, seed=99)
        print(f"{k:>3} indicators: strict {s:>7.2%}   <=1 invalid {o:>7.2%}")


if __name__ == "__main__":
    main()
