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

### Rust Tests

Tests are located in:
- Unit tests: `#[cfg(test)] mod tests` within each module
- Integration tests: `src-tauri/src/core/grouping/real_data_test.rs` (uses actual test data)

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
- `src-tauri/src/core/stats/*/tests`: Statistical test validation
- `src-tauri/src/core/grouping/tests.rs`: Algorithm unit tests
- `src-tauri/src/core/exporter_test.rs`: Export format validation

### Test Data

**Location:** `通用动物实验自动分组软件_测试用数据.xlsx` (root directory)
- 10 animals (6 male, 4 female)
- 73 indicators
- Multi-row headers (Row 1: English names, Row 2: Chinese names + units)

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

The export must use **dual-row header format** to match regulatory requirements:
- **Row 1:** Group labels + Animal IDs (merged cells per animal)
- **Row 2:** Sex labels ("雄性" or "雌性")
- **Rows 3+:** Indicator names (Chinese) + values

**Never** export single-row headers with "组别 | 动物编号 | 性别" - this is an outdated format.

See `src-tauri/src/core/exporter.rs` for the canonical implementation.

### Statistical Engine

All statistical tests are **pure Rust** implementations (no Python/R dependencies):
- Validated against scipy/statsmodels in development
- Use `statrs` crate for distributions, but custom implementations for:
  - Levene test (Brown-Forsythe variant)
  - Welch ANOVA
  - Tukey HSD
  - Dunnett's T3

### Performance Characteristics

- **Enumeration algorithm:** Suitable for ≤50 animals (current test data: 10 animals)
- **Parallel evaluation:** Uses `rayon` to evaluate candidates concurrently
- **Expected performance:** < 1s for test data (10 animals, 73 indicators, 2 groups)

For future scaling (>50 animals), Monte Carlo sampling should be implemented.

## Documentation

**Primary docs:** `docs/` directory
- `README.md`: Architecture overview
- `technical_specification.md`: Requirements and design decisions
- `implementation_design.md`: Detailed code structure
- `data_format_spec.md`: Excel parsing rules
- `output_format_spec.md`: Export format specification
- `PROGRESS_SUMMARY.md`: Development status (85% backend, 0% frontend as of 2026-02-12)

**Progress reports:** `docs/*_completion.md` files document completed phases

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
3. Update `evaluator.rs` for scoring/filtering changes
4. Run `cargo test grouping` to validate
5. Test with real data: `cargo test real_data_test`

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
5. **Test data paths:** Hardcoded test data path in `real_data_test.rs` may need adjustment on different machines

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
