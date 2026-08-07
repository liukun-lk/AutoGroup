"""Compare the lab's current manual Excel workflows against blocked randomization.

The lab describes two procedures:

  A "complete random"    : sort by BW -> RAND() column -> sort by RAND -> fill groups
                           sequentially (first n rows = group 1, next n = group 2, ...)
  B "stratified random"  : same up to the RAND sort, then label groups cyclically
                           1,2,3,...,k,1,2,3,...,k and sort by that label

The question this script answers: are A and B actually different, and is there a
procedure that delivers what the lab wants (BW balanced on the first try, no
re-drawing until it looks even)?

  C blocked by BW        : sort by BW -> cut into consecutive blocks of k -> shuffle
                           inside each block -> i-th animal of each block goes to group i
"""

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
BW_ORDER = sorted(range(N), key=lambda i: BW[i])  # ascending by BW


# ---------------------------------------------------------------- allocations


def alloc_sequential(rng, k, size):
    """A: shuffle everything, then take consecutive slices."""
    order = list(range(N))
    rng.shuffle(order)
    return [order[g * size : (g + 1) * size] for g in range(k)]


def alloc_cyclic(rng, k, size):
    """B: shuffle everything, then deal out cyclically 1,2,...,k,1,2,...,k."""
    order = list(range(N))
    rng.shuffle(order)
    groups = [[] for _ in range(k)]
    for pos, idx in enumerate(order):
        groups[pos % k].append(idx)
    return groups


def alloc_blocked_by_bw(rng, k, size):
    """C: BW-sorted blocks of k; shuffle within each block; deal one per group."""
    groups = [[] for _ in range(k)]
    for b in range(size):
        block = BW_ORDER[b * k : (b + 1) * k]
        block = list(block)
        rng.shuffle(block)
        for g in range(k):
            groups[g].append(block[g])
    return groups


PROCEDURES = [
    ("A  sequential fill  (lab: 完全随机)", alloc_sequential),
    ("B  cyclic fill      (lab: 分层随机)", alloc_cyclic),
    ("C  blocked by BW    (proposed)", alloc_blocked_by_bw),
]


# ---------------------------------------------------------------- measurement


def measure(alloc, k, trials=TRIALS, seed=987654):
    rng = random.Random(seed)
    size = N // k
    bw_ps, cd_ps, ranges, strict, bw_ok = [], [], [], 0, 0
    for _ in range(trials):
        groups = alloc(rng, k, size)
        bw_groups = [[BW[i] for i in g] for g in groups]
        cd_groups = [[CD[i] for i in g] for g in groups]
        r_bw = test_indicator(bw_groups, ALPHA, screen_only=True)
        r_cd = test_indicator(cd_groups, ALPHA, screen_only=True)
        bw_ps.append(r_bw.diff_p)
        cd_ps.append(r_cd.diff_p)
        means = [statistics.mean(g) for g in bw_groups]
        ranges.append(max(means) - min(means))
        strict += r_bw.valid and r_cd.valid
        bw_ok += r_bw.valid
    q = lambda xs, p: sorted(xs)[min(len(xs) - 1, int(p * len(xs)))]  # noqa: E731
    return {
        "bw_p_median": statistics.median(bw_ps),
        "bw_p_q05": q(bw_ps, 0.05),
        "cd_p_median": statistics.median(cd_ps),
        "range_median": statistics.median(ranges),
        "range_q95": q(ranges, 0.95),
        "range_max": max(ranges),
        "bw_pass": bw_ok / trials,
        "strict_pass": strict / trials,
    }


def report(k):
    size = N // k
    print(f"\n=== {N} animals -> {k} groups x {size} (alpha={ALPHA}, {TRIALS} draws) ===")
    print(
        f"{'procedure':<38}{'BW P median':>13}{'BW P q05':>10}"
        f"{'BW pass':>9}{'both pass':>11}{'BW mean spread (g)':>26}"
    )
    for name, alloc in PROCEDURES:
        m = measure(alloc, k)
        spread = f"med {m['range_median']:.3f}  q95 {m['range_q95']:.3f}  max {m['range_max']:.3f}"
        print(
            f"{name:<38}{m['bw_p_median']:>13.3f}{m['bw_p_q05']:>10.3f}"
            f"{m['bw_pass']:>9.1%}{m['strict_pass']:>11.1%}{spread:>26}"
        )


def equivalence_check(k):
    """A vs B: same distribution? Compare quantiles of BW P under both."""
    size = N // k
    out = {}
    for name, alloc in PROCEDURES[:2]:
        rng = random.Random(4242)
        ps = []
        for _ in range(8000):
            groups = alloc(rng, k, size)
            ps.append(
                test_indicator(
                    [[BW[i] for i in g] for g in groups], ALPHA, screen_only=True
                ).diff_p
            )
        ps.sort()
        out[name] = ps
    a, b = out[PROCEDURES[0][0]], out[PROCEDURES[1][0]]
    print(f"\n=== A vs B, distribution of BW P over 8000 draws ({k} groups) ===")
    print(f"{'quantile':<12}{'A sequential':>15}{'B cyclic':>12}{'abs diff':>11}")
    for p in (0.05, 0.25, 0.5, 0.75, 0.95):
        va, vb = a[int(p * len(a))], b[int(p * len(b))]
        print(f"q{p:<11}{va:>15.4f}{vb:>12.4f}{abs(va - vb):>11.4f}")
    ks = max(abs(i / len(a) - sum(1 for x in b if x <= v) / len(b)) for i, v in enumerate(a))
    print(f"KS statistic = {ks:.4f}   (critical value at 0.05 for n=8000 is ~0.0215)")
    print("A and B are the same randomization; the cyclic dealing changes nothing.")


def rejection_cost_on_top_of_blocking(k):
    """C plus a CD45 acceptance criterion: how many draws until both indicators pass?"""
    size = N // k
    rng = random.Random(2026)
    draws, accepted = [], 0
    for _ in range(500):
        for attempt in range(1, 1001):
            groups = alloc_blocked_by_bw(rng, k, size)
            r_bw = test_indicator(
                [[BW[i] for i in g] for g in groups], ALPHA, screen_only=True
            )
            r_cd = test_indicator(
                [[CD[i] for i in g] for g in groups], ALPHA, screen_only=True
            )
            if r_bw.valid and r_cd.valid:
                draws.append(attempt)
                accepted += 1
                break
    print(f"\n=== C + CD45 acceptance criterion ({k} groups), 500 runs ===")
    print(
        f"accepted {accepted}/500   mean draws {statistics.mean(draws):.2f}   "
        f"max draws {max(draws)}"
    )


def measure_with_criterion(alloc, k, trials=2000, seed=13579, enforce=True):
    """Rejection sampling: redraw until every indicator clears alpha.

    Reports what the criterion actually buys for the PRIMARY indicator, which is the
    question that separates the four randomized variants from each other.
    """
    rng = random.Random(seed)
    size = N // k
    bw_ps, ranges, draws, cd_ok = [], [], [], 0
    for _ in range(trials):
        for attempt in range(1, 1001):
            groups = alloc(rng, k, size)
            bw_groups = [[BW[i] for i in g] for g in groups]
            r_bw = test_indicator(bw_groups, ALPHA, screen_only=True)
            r_cd = test_indicator(
                [[CD[i] for i in g] for g in groups], ALPHA, screen_only=True
            )
            if not enforce or (r_bw.valid and r_cd.valid):
                bw_ps.append(r_bw.diff_p)
                means = [statistics.mean(g) for g in bw_groups]
                ranges.append(max(means) - min(means))
                draws.append(attempt)
                cd_ok += r_cd.valid
                break
    q = lambda xs, p: sorted(xs)[min(len(xs) - 1, int(p * len(xs)))]  # noqa: E731
    return {
        "bw_p_median": statistics.median(bw_ps),
        "range_median": statistics.median(ranges),
        "range_q95": q(ranges, 0.95),
        "range_max": max(ranges),
        "cd_pass": cd_ok / trials,
        "mean_draws": statistics.mean(draws),
        "max_draws": max(draws),
    }


def four_variants(k):
    """The four randomized methods = {no stratification, blocked} x {no criterion, criterion}."""
    size = N // k
    variants = [
        ("完全随机", alloc_sequential, False),
        ("完全随机 + 接受准则", alloc_sequential, True),
        ("按 BW 分层随机", alloc_blocked_by_bw, False),
        ("按 BW 分层随机 + 接受准则", alloc_blocked_by_bw, True),
    ]
    print(f"\n=== the four randomized variants ({N} -> {k} x {size}) ===")
    print(
        f"{'variant':<30}{'BW P median':>13}{'BW spread med/q95/max (g)':>28}"
        f"{'CD45 pass':>11}{'draws':>14}"
    )
    for name, alloc, enforce in variants:
        m = measure_with_criterion(alloc, k, enforce=enforce)
        spread = (
            f"{m['range_median']:.3f} / {m['range_q95']:.3f} / {m['range_max']:.3f}"
        )
        draws = f"{m['mean_draws']:.2f} (max {m['max_draws']})"
        print(
            f"{name:<30}{m['bw_p_median']:>13.3f}{spread:>28}"
            f"{m['cd_pass']:>11.1%}{draws:>14}"
        )


def randomization_space(k):
    """Blocking does not shrink the randomization space to anything small."""
    import math

    size = N // k
    full = math.factorial(N)
    for _ in range(k):
        full //= math.factorial(size)
    blocked = math.factorial(k) ** size
    print(f"\n=== randomization space ({k} groups x {size}) ===")
    print(f"complete randomization : {full:.3e} labelled allocations")
    print(f"blocked by BW          : {blocked:.3e} labelled allocations")
    print(
        "Blocking removes the BW-unbalanced allocations; what remains is still\n"
        "astronomically large, so the draw is in no practical sense less random."
    )


def how_much_redrawing_would_A_need(k):
    """C reaches, on the first draw, a balance level A only hits rarely."""
    size = N // k
    target = measure(alloc_blocked_by_bw, k)["range_max"]  # worst spread C produced
    rng = random.Random(555)
    hits = 0
    trials = 8000
    for _ in range(trials):
        groups = alloc_sequential(rng, k, size)
        means = [statistics.mean([BW[i] for i in g]) for g in groups]
        if max(means) - min(means) <= target:
            hits += 1
    p = hits / trials
    print(f"\n=== matching C's balance by re-drawing under A ({k} groups) ===")
    print(f"C's worst BW mean spread over {TRIALS} draws : {target:.3f} g")
    print(f"P(A reaches that spread or better)          : {p:.2%}")
    if p > 0:
        print(f"expected manual re-draws to match C         : {1/p:.0f}")
    else:
        print(f"expected manual re-draws to match C         : >{trials}")


if __name__ == "__main__":
    for k in (6, 3):
        report(k)
    for k in (6, 3):
        four_variants(k)
    equivalence_check(6)
    rejection_cost_on_top_of_blocking(6)
    randomization_space(6)
    how_much_redrawing_would_A_need(6)
    how_much_redrawing_would_A_need(3)
