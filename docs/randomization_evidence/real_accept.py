"""Acceptance rate of a purely random allocation on the REAL 60-animal dataset.

Answers, for each plausible group layout:
  - how often a random split already passes the balance cascade (= risk of plan A)
  - the expected number of draws for rejection sampling (= cost of plan B)
  - how the best-of-100k optimized pick compares to a random pick (data-dredging gap)
"""

import math
import random
import statistics
import sys

sys.path.insert(0, ".claude/skills/animal-grouping/scripts")
from grouping_engine import read_xlsx, test_indicator  # noqa: E402

PATH = "src-tauri/tests/fixtures/randomization_input_60f.xlsx"
ALPHA = 0.05
TRIALS = 2000

rows = read_xlsx(PATH)[2:]  # dual-row header
BW = [r[2] for r in rows]
CD = [r[3] for r in rows]
N = len(rows)


def run(indicators, sizes, trials=TRIALS, seed=20260806):
    """Return (strict_rate, optimized_rate, min_p_stats, per_indicator_fail_counts)."""
    rng = random.Random(seed)
    order = list(range(N))
    strict = opt = 0
    min_ps = []
    fails = [0] * len(indicators)
    for _ in range(trials):
        rng.shuffle(order)
        n_invalid = 0
        min_p = 1.0
        for k, values in enumerate(indicators):
            groups, pos = [], 0
            for size in sizes:
                groups.append([values[i] for i in order[pos : pos + size]])
                pos += size
            res = test_indicator(groups, ALPHA, screen_only=True)
            if not res.valid:
                n_invalid += 1
                fails[k] += 1
            min_p = min(min_p, res.diff_p)
        min_ps.append(min_p)
        strict += n_invalid == 0
        opt += n_invalid <= 1
    return strict / trials, opt / trials, min_ps, fails


def combinations_count(sizes):
    total = math.factorial(sum(sizes))
    for s in sizes:
        total //= math.factorial(s)
    return total


LAYOUTS = [
    ("3 groups x 20", [20, 20, 20]),
    ("4 groups x 15", [15, 15, 15, 15]),
    ("5 groups x 12", [12, 12, 12, 12, 12]),
    ("6 groups x 10", [10, 10, 10, 10, 10, 10]),
    ("3 groups x 18 + reserve 6", [18, 18, 18]),  # reserve excluded from stats
]

print(f"real data: n={N}, all female, indicators = BW(g), CD45%   alpha={ALPHA}")
print(f"{TRIALS} independent random allocations per layout\n")
print(
    f"{'layout':<28}{'labelled splits':>18}{'strict pass':>13}"
    f"{'<=1 invalid':>13}{'draws for B':>13}{'median min(P)':>15}"
)
for name, sizes in LAYOUTS:
    # Shuffle all N animals; the first sum(sizes) fill the experimental groups and
    # whatever is left over is the reserve group, which never enters the statistics.
    s, o, min_ps, fails = run([BW, CD], sizes)
    space = combinations_count(sizes)
    draws = f"{1/s:.2f}" if s > 0 else "inf"
    print(
        f"{name:<28}{space:>18.2e}{s:>13.1%}{o:>13.1%}{draws:>13}"
        f"{statistics.median(min_ps):>15.3f}"
    )

print("\nwhich indicator fails, 3 groups x 20:")
s, o, min_ps, fails = run([BW, CD], [20, 20, 20])
print(f"  BW  invalid in {fails[0]/TRIALS:.1%} of random splits")
print(f"  CD45% invalid in {fails[1]/TRIALS:.1%} of random splits")

print("\nmin(P) distribution over random splits (3 x 20):")
sp = sorted(min_ps)
for q in (0.01, 0.05, 0.25, 0.5, 0.75, 0.95, 0.99, 1.0):
    print(f"  q{q:<5} = {sp[min(len(sp)-1, int(q*len(sp)))]:.4f}")
print(f"  best of {TRIALS} random draws = {sp[-1]:.4f}")
print(
    "\nThe optimizer keeps the max over ~100k draws, i.e. far into this right tail:\n"
    "  that is the number a reviewer would see under the current Optimized mode."
)
