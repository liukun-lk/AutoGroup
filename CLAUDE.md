# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**AutoGroup** is a desktop application for automated animal grouping in laboratory experiments. It uses statistical balance algorithms to assign animals to experimental groups while maintaining balanced distributions across multiple biological indicators.

**Tech Stack:**
- **Frontend:** React 19 + TypeScript + Vite + Tailwind CSS 4
- **Backend:** Rust (Tauri 2.x)
- **Build:** Bun (package manager), Cargo (Rust)

**Core Purpose:** Replace manual animal grouping by ensuring statistical balance (all indicators P > α) across groups, saving researchers 80% of grouping time while ensuring reproducibility.

## Architecture

### High-Level Structure

```
Frontend (React)              Backend (Rust)
    ├─ UI Components    →     ├─ Commands Layer (Tauri IPC)
    ├─ State (Jotai)    →     ├─ Core Engine
    └─ Tauri API calls  →     │   ├─ parser: Excel parsing
                              │   ├─ grouping: Algorithm
                              │   ├─ stats: Statistical tests
                              │   ├─ validator: Data validation
                              │   └─ exporter: Excel export
                              ├─ Persistence (SQLite)
                              └─ Utils
```

### Backend Module Organization

The Rust backend (`src-tauri/src/`) follows a layered architecture:

1. **Commands** (`commands/`): Tauri IPC handlers
   - `import.rs`: `parse_excel` - Excel file parsing
   - `grouping.rs`: `compute_grouping` - Main algorithm invocation
   - `export.rs`: `export_result` - Excel export

2. **Core Engine** (`core/`):
   - `models.rs`: Core data structures (`Animal`, `Dataset`, `GroupingResult`, `Sex`, etc.)
   - `parser.rs`: Excel parsing with multi-row header support (73 indicators)
   - `validator.rs`: Data validation (uniqueness, completeness)
   - `grouping/`: Grouping algorithm
     - `enumerator.rs`: Combination enumeration (≤50 animals)
     - `evaluator.rs`: Candidate evaluation with parallel stats
   - `stats/`: Pure Rust statistical tests
     - `levene.rs`: Variance homogeneity test
     - `ttest.rs`: Student's & Welch's t-tests
     - `anova.rs`: One-way & Welch ANOVA
     - `tukey.rs`: Tukey HSD post-hoc test
     - `dunnett.rs`: Dunnett's T3 post-hoc test
     - `distributions.rs`: Studentized range & maximum modulus (post-hoc P values)
   - `exporter.rs`: Excel export with dual-row headers

3. **Persistence** (`persistence/`): SQLite repositories for config and history

4. **Utils** (`utils/`): Error handling and utilities

### Key Data Flow

1. **Import:** User uploads Excel → `parse_excel()` → `Dataset` (animals + metadata)
2. **Configuration:** User sets group constraints + stat params
3. **Computation:** `compute_grouping()` → enumerate candidates → evaluate in parallel (rayon) → select best
4. **Export:** `export_result()` → generate Excel with dual-row header format

### Critical Design Patterns

**Statistical Test Selection:**
```rust
// For each indicator, select appropriate test based on:
// 1. Number of groups (2 → t-test, ≥3 → ANOVA)
// 2. Variance homogeneity (Levene test)
if num_groups == 2 {
    if levene_p > α { student_ttest() } else { welch_ttest() }
} else {
    if levene_p > α { anova + tukey_hsd() } else { welch_anova + dunnett_t3() }
}
```

**Optimization Modes:**
- **Strict:** All selected indicators must have P > α
- **Optimized:** Allow max 1 indicator with P ≤ α; maximize min(P), then mean(P)

**Sex Handling:**
- Internal representation: `Sex::Male`, `Sex::Female`
- Export format: Must convert to Chinese ("雄性", "雌性") using `Sex::to_chinese()`

## Development Commands

### Frontend (React)

```bash
# Install dependencies
bun install

# Development server (starts both frontend and Tauri)
bun run tauri dev

# Build TypeScript
bun run build  # Runs: tsc && vite build

# Preview production build
bun run preview
```

### Backend (Rust)

**Working Directory:** All Rust commands must be run from `src-tauri/` directory

```bash
cd src-tauri

# Build backend only
cargo build

# Build release
cargo build --release

# Run all tests
cargo test

# Run specific test module
cargo test stats::ttest::tests
cargo test grouping::tests

# Run tests with output
cargo test -- --nocapture

# Format code
cargo fmt

# Lint
cargo clippy
cargo clippy -- -D warnings  # Fail on warnings

# Check without building
cargo check
```

### Full Stack

```bash
# Development (from project root)
bun run tauri dev

# Production build
bun run tauri build
```

## Testing Strategy

### Required check on every change (do not skip)

**Every code change in this repository — Rust or TypeScript, feature or refactor, one line or
one thousand — must pass the following before it can be called done.** Run them and read the
output; never claim a change works without having done so.

```bash
cd src-tauri
cargo fmt                       # must be run before committing
cargo test --release            # all tests, including the end-to-end golden test
cargo clippy --all-targets      # must not add warnings on top of the existing baseline
cd .. && bun run build          # tsc + vite build
```

If a change touches `core/grouping/`, `core/stats/`, `core/parser.rs` or `core/exporter.rs`,
also run the slow suites:

```bash
cd src-tauri && cargo test --release -- --ignored   # real-data + performance harnesses
```

The single most important gate is the end-to-end test described below. It is the only test
that checks the whole pipeline against output a human actually accepted.

### End-to-end golden test (`tests/e2e_grouping_test.rs`)

```
tests/fixtures/e2e_input.xlsx
  → parse → compute grouping → export
  → compared cell by cell against tests/fixtures/e2e_expected_output.xlsx
```

Both fixtures are real application artifacts; only the animal IDs were anonymized (the study
code became a `DEMO0xx` prefix) and the document properties scrubbed. Every measurement,
header, sheet and cell is untouched. The expected grouping was independently confirmed to be
the statistical optimum by the exact Python reference implementation in
`.claude/skills/animal-grouping/`.

It asserts, in order:

1. **Parse** — 9 animals (6M + 3F), 73 indicator keys from the dual-row header
2. **Indicator filtering** — 70 numeric indicators after dropping the 3 text columns
   (`样本号`, `样品识别号`, `FULLNAME`), matching `src/utils/indicator-filter.ts`
3. **Grouping** — the winning assignment must match the accepted one animal by animal, with
   all 70 indicators passing at α = 0.05 in Strict mode
4. **Post-hoc** — each of the 70 indicators carries all C(3,2) = 3 pairwise comparisons and
   all of them clear α, plus a guard that they are not saturated at exactly 1.0 (the
   signature of the approximation that used to stand in for the studentized range)
5. **Export** — every cell of all four sheets (`分组结果` / `统计结果` / `事后比较` /
   `汇总信息`). Text compares exactly; numbers compare to a `1e-9` relative tolerance so
   cross-platform libm differences don't cause false failures. Only the `计算耗时 (ms)` row
   is skipped.

**If this test fails, assume the code regressed — not that the fixture is stale.** The failure
message names the sheet, row, column and both values. Only regenerate the fixture when you
*intentionally* changed the output format, and say so explicitly in the commit message. See
`src-tauri/tests/fixtures/README.md` for provenance and the regeneration procedure.

### Rust Tests

Tests are located in:
- Unit tests: `#[cfg(test)] mod tests` within each module
- Integration tests: `src-tauri/src/core/grouping/real_data_test.rs` (uses actual test data)
- End-to-end test: `src-tauri/tests/e2e_grouping_test.rs` (see above)

**Running specific test categories:**
```bash
cd src-tauri

# Statistical engine tests
cargo test stats

# Grouping algorithm tests
cargo test grouping

# Real data validation test
cargo test real_data_test
```

**Key test files:**
- `src-tauri/tests/e2e_grouping_test.rs`: whole-pipeline golden test (highest-value gate)
- `src-tauri/src/core/stats/*/tests`: Statistical test validation
- `src-tauri/src/core/grouping/tests.rs`: Algorithm unit tests, including
  `test_top_candidates_match_full_evaluation` — proves the two-pass scoring pipeline returns
  exactly what fully evaluating every candidate and sorting would return
- `src-tauri/src/core/grouping/perf_repro.rs`: `#[ignore]`d performance harness guarding the
  ≥3-group blow-up (see Performance Characteristics)
- `src-tauri/src/core/exporter_test.rs`: Export format validation

### Test Data

**End-to-end fixtures:** `src-tauri/tests/fixtures/` (anonymized; see its `README.md`)
- `e2e_input.xlsx`: 9 animals (6 male, 3 female), 73 parsed indicator keys → 70 tested
- `e2e_expected_output.xlsx`: the accepted 3-group export for that input

**Legacy sample:** `docs/通用动物实验自动分组软件_测试用数据.xlsx`
- 9 animals (6 male, 3 female)
- 71 parsed indicator keys, of which 3 (`样本号`, `样品识别号`, `FULLNAME`) are text columns and
  never enter the statistics, so "all indicators" means 68 tested
- Multi-row headers (Row 1: English names, Row 2: Chinese names + units)

All test data paths are resolved from `env!("CARGO_MANIFEST_DIR")`, so tests run from any
checkout. Never reintroduce an absolute path — a test that silently skips protects nothing.

## Code Style & Conventions

### Rust

- **Formatting:** Must use `cargo fmt` before committing
- **Linting:** Code should pass `cargo clippy` without warnings
- **Naming:** Standard Rust `snake_case` for functions/variables, `PascalCase` for types
- **Error handling:** Use `anyhow::Result` for functions, `thiserror` for custom errors
- **Comments:** English only in code; Chinese for user-facing strings
- **Parallelization:** Use `rayon` for CPU-intensive operations (grouping evaluation)

### TypeScript

- **Formatting:** Standard Vite/React conventions
- **Naming:** camelCase for variables/functions, PascalCase for components
- **Types:** Strict TypeScript; mirror Rust types in `src/types/`

### Git Commits

**Follow Conventional Commits (from `.cursorrules`):**

```
<type>(<scope>): <subject>

[optional body]
```

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `chore`, `ci`
**Scopes:** `frontend`, `backend`, `ui`, `db`, `ai`, `stats`, `grouping`, `export`, `docs`
**Subject rules:**
- Imperative mood: "add" not "added"
- Lowercase first letter
- No period at end
- Max 72 characters
- English only

**Examples:**
```
feat(stats): add Dunnett's T3 post-hoc test
fix(export): resolve dual-row header alignment issue
docs: update CLAUDE.md with testing commands
refactor(grouping): simplify evaluator scoring logic
```

## Important Implementation Notes

### Excel Export Format (Critical)

`分组结果` is one row per animal, under a two-row header:
- **Row 1:** units, aligned over the indicator columns (blank over the label columns)
- **Row 2:** `组别 | 动物编号 | 性别` [`| 区组 | 随机数`] `|` indicator display names
- **Rows 3+:** one row per animal, plus a `均值±标准差` row after each experimental group

This is the format in `tests/fixtures/e2e_expected_output.xlsx`, which is a real export the
user accepted; earlier revisions of this file described a transposed layout with animals as
columns, which neither the code nor the accepted artifact has ever produced.

**`区组` and `随机数` are audit columns and appear only for the randomized methods** (`随机数`
whenever a draw was recorded, `区组` only under blocked randomization). They are not
decoration: the allocation *is* "sort by the draw inside the block, then deal each group its
quota in turn", so a reviewer can re-sort the sheet in Excel and reproduce `组别` by hand —
the same check the lab used to do with a `RAND()` column, except reproducible from a seed.
`the_exported_sheet_can_be_re_sorted_into_the_same_grouping` in the e2e test performs exactly
that replay against the written workbook, and `randomizer/tests.rs` does it at the model
level. Optimization records no draw, so its export keeps the plain layout and the golden
fixture is unaffected.

Sheets written, in order: `分组结果`, then `统计结果` and `事后比较` when
`include_statistics` is set, then `汇总信息` when `include_summary` is set. `统计结果` keeps
its five columns (indicator, Levene P, main test P, method, verdict); the pairwise post-hoc
comparisons live in `事后比较` as one row per (indicator, group pair), which reviewers read
the way they read a GraphPad multiple-comparisons table. That sheet is omitted entirely for
two-group designs, which have no post-hoc stage.

See `src-tauri/src/core/exporter.rs` for the canonical implementation.

### Statistical Engine

All statistical tests are **pure Rust** implementations (no Python/R dependencies):
- Use `statrs` crate for distributions, but custom implementations for:
  - Levene test (mean-centered, i.e. the original Levene — not Brown-Forsythe, despite what
    earlier notes claimed)
  - Welch ANOVA
  - Tukey HSD — exact, via the studentized range distribution in `stats/distributions.rs`
  - Dunnett's T3 — exact, via the studentized maximum modulus correction in the same module

`stats/distributions.rs` is a direct port of `srange_sf` / `smm_sf` / `_chi_scale_integral`
from the Python reference implementation, checked against published critical value tables
(q₀.₀₅(3,12) = 3.773, q₀.₀₅(4,20) = 3.958, q₀.₀₅(5,10) = 4.654) and against the analytic
identities at k = 2 / C = 1. On the e2e fixture all 210 pairwise comparisons agree with the
Python reference to within 1.8e-11. **Do not replace these with a t-distribution
approximation** — the previous `q/sqrt(2) * k` shortcut pinned every comparison at exactly
1.000000 on small samples, and the uncorrected Welch t used for T3 returned p-values ~0.2-0.4x
the true ones. `tukey.rs` and `dunnett.rs` carry regression tests for both failure modes.

**Zero-variance splits must never reach a distribution.** When every group is internally
constant the F and t statistics degenerate to 0/0, and `statrs` panics on a NaN argument —
this took whole runs down. `one_way_anova` (which Levene also calls), both t-tests and
`tukey.rs::pairwise` therefore short-circuit when the spread is zero and answer from the means
alone: equal means → P = 1, different means → P = 0. These guards are literal ports of the
reference implementation and each carries a regression test. Welch ANOVA and Dunnett's T3 stay
undefined for such input (`Err`; the reference returns NaN) — see `references/statistics.md` in
the grouping skill for the full table and what the engine does with each outcome.

Post-hoc p-values cost a quadrature each, so the cascade takes a `PostHocDetail`: the scoring
pass asks for `ValidityOnly` (a cached critical value for Tukey, a bounds-based shortcut for
T3) and only the reported Top-N candidates compute `Exact` values. Both routes give the same
verdict by construction; `tukey_all_valid_agrees_with_exact_p_values` and its Dunnett twin
guard that.

For any doubt about a specific P value, cross-check with the exact reference implementation:
`python3 .claude/skills/animal-grouping/scripts/grouping_engine.py verify --help`.

### Performance Characteristics

- **Enumeration algorithm:** Suitable for ≤50 animals (current test data: 9 animals)
- **Parallel evaluation:** Uses `rayon` to evaluate candidates concurrently
- **Expected performance:** ~1–2 s and well under 200 MB for a 3-group, 46-indicator run over
  100 000 candidates

Monte Carlo sampling kicks in above 500 000 combinations (`enumerator.rs`). It draws from a
`ChaCha12Rng` handed in by the caller, not from `thread_rng`, so a run above that threshold
reproduces itself. `compute_optimal_grouping` seeds it from the fixed `OPTIMIZED_SAMPLING_SEED`
(optimization takes no seed from the user, but its sampling still has to be reproducible or the
exported result cannot be recomputed). Never put `thread_rng` back on this path —
`optimized_sampling_path_is_reproducible` in `grouping/tests.rs` guards it.

**Do not reintroduce per-candidate result materialization.** `compute_optimal_grouping` scores
every candidate with an allocation-free `CandidateScore` and builds the full `GroupingResult`
only for the Top-N winners. Building one per candidate costs ~13 KB each, which at the 10^5–10^6
candidates that ≥3 groups produce meant 1.5 GB+ of live heap and a run that never appeared to
finish on Windows. `perf_repro.rs` guards this; run it after touching the evaluation path:

```bash
cd src-tauri && cargo test --release perf_ -- --nocapture --ignored
```

## Documentation

**Primary docs:** `docs/` directory
- `README.md`: Architecture overview
- `technical_specification.md`: Requirements and design decisions
- `implementation_design.md`: Detailed code structure
- `data_format_spec.md`: Excel parsing rules
- `output_format_spec.md`: Export format specification
- `PROGRESS_SUMMARY.md`: Development status (85% backend, 0% frontend as of 2026-02-12)

**Progress reports:** `docs/*_completion.md` files document completed phases

**Grouping engine spec:** `.claude/skills/animal-grouping/` is the authoritative reference for the
grouping algorithm — the five-stage contract, the test-selection cascade, the validity rule, known
edge cases, and a zero-dependency Python reference implementation
(`scripts/grouping_engine.py`) that computes exact P values and can audit the Rust engine's output.
Consult it before changing anything under `src-tauri/src/core/grouping/` or `.../stats/`, and before
answering "is this grouping balanced / is this P value right".

## Common Workflows

### Adding a New Statistical Test

1. Create module in `src-tauri/src/core/stats/`
2. Implement function with signature: `fn test_name(groups: &[&[f64]]) -> Result<f64>`
3. Add unit tests comparing to known results
4. Integrate into `evaluator.rs` selection logic
5. Update `StatisticalTest` enum in `models.rs` if needed

### Modifying Grouping Logic

1. Core algorithm: `src-tauri/src/core/grouping/mod.rs::compute_optimal_grouping`
2. Update `enumerator.rs` for candidate generation changes
3. Update `evaluator.rs` for scoring/filtering changes — keep the scoring pass and the full
   evaluation sharing one indicator loop, so ranking numbers and reported numbers cannot diverge
4. Run `cargo test grouping` to validate
5. Test with real data: `cargo test real_data_test -- --ignored`
6. **Run the end-to-end test:** `cargo test --test e2e_grouping_test`
7. Check performance did not regress: `cargo test --release perf_ -- --nocapture --ignored`
8. For any question about whether a P value or a grouping is correct, cross-check with the exact
   Python reference implementation in `.claude/skills/animal-grouping/` — do not reason it out

### Adding Tauri Commands

1. Create handler in `src-tauri/src/commands/<module>.rs`
2. Add to `invoke_handler!` in `src-tauri/src/lib.rs`
3. Mirror TypeScript types in `src/types/`
4. Add frontend wrapper in `src/lib/tauri.ts`

## Known Constraints & Gotchas

1. **Working directory:** Rust commands MUST be run from `src-tauri/` directory
2. **Sex conversion:** Always use `Sex::to_chinese()` for export; never hardcode "M"/"F"
3. **Indicator metadata:** Use `IndicatorMetadata.key` for lookups, `display_name` for UI/export
4. **Parallel safety:** Grouping evaluation is CPU-bound and thread-safe (uses `rayon::par_iter`)
5. **Test data paths:** Resolve every fixture from `env!("CARGO_MANIFEST_DIR")`; absolute paths
   make tests skip silently on other machines
6. **Reserve groups:** A reserve group is only created when it actually holds animals, and it is
   excluded from `ResultSummary.num_groups`, from the statistics, and from the exported group
   count. An empty reserve group must never be sent to the backend

## Dependencies

### Key Rust Crates

- `tauri` (2.x): Desktop framework
- `calamine` (0.26): Excel parsing
- `statrs` (0.18): Statistical distributions
- `special` (0.11): Special functions (for Dunnett's T3)
- `rusqlite` (0.32): SQLite database
- `rayon` (1.10): Parallel computing
- `rust_xlsxwriter` (0.83): Excel export
- `anyhow` (1.0): Error handling
- `serde` + `serde_json`: Serialization

### Key Frontend Dependencies

- `react` (19.1.0)
- `@tauri-apps/api` (2.x): Tauri frontend APIs
- `vite` (7.x): Build tool
- `tailwindcss` (4.x): CSS framework

## Future Enhancements (Not Yet Implemented)

- Frontend UI (0% complete as of 2026-02-12)
- SQLite persistence (structure defined, not integrated)
- Config template save/load
- Grouping history viewer
- Monte Carlo sampling for large datasets (>50 animals)
- P-value distribution visualization (ECharts)
