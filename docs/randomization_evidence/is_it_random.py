"""Could the current engine's output have come from a random draw?

The engine reported min_p = 0.998319 for the 3 x 20 layout on the real data.
If the allocation were produced by (stratified) random sampling, min_p would be a
single draw from the distribution below. This measures how far out that value sits.
"""

import random
import statistics
import sys

sys.path.insert(0, ".claude/skills/animal-grouping/scripts")
from grouping_engine import read_xlsx, test_indicator  # noqa: E402

PATH = "src-tauri/tests/fixtures/randomization_input_60f.xlsx"
ALPHA = 0.05
TRIALS = 100_000
ENGINE_MIN_P = 0.998319  # what compute_optimal_grouping actually returned (3 x 20)

rows = read_xlsx(PATH)[2:]  # dual-row header
BW = [r[2] for r in rows]
CD = [r[3] for r in rows]
N = len(rows)
SIZES = [20, 20, 20]

rng = random.Random(31337)
order = list(range(N))
min_ps = []
for _ in range(TRIALS):
    rng.shuffle(order)
    groups, pos = [], 0
    for s in SIZES:
        groups.append(order[pos : pos + s])
        pos += s
    p_bw = test_indicator([[BW[i] for i in g] for g in groups], ALPHA, screen_only=True).diff_p
    p_cd = test_indicator([[CD[i] for i in g] for g in groups], ALPHA, screen_only=True).diff_p
    min_ps.append(min(p_bw, p_cd))

min_ps.sort()
print(f"{TRIALS} random allocations, 60 -> 3 x 20, min(P) over BW and CD45%")
print(f"  median          {statistics.median(min_ps):.4f}")
for q in (0.5, 0.9, 0.99, 0.999, 0.9999):
    print(f"  q{q:<8}      {min_ps[min(len(min_ps)-1, int(q*len(min_ps)))]:.4f}")
print(f"  maximum         {min_ps[-1]:.4f}")
print()
for thr in (0.95, 0.99, 0.995, ENGINE_MIN_P):
    hits = sum(1 for p in min_ps if p >= thr)
    label = "  <- engine's output" if thr == ENGINE_MIN_P else ""
    print(f"P(min(P) >= {thr:.6f}) = {hits}/{TRIALS} = {hits/TRIALS:.5%}{label}")
