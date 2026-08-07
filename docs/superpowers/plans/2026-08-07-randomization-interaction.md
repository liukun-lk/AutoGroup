# Randomization Interaction (acceptance tiers, candidate switching, reproduction card) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `docs/randomization_interaction_design.md`: a calibrated top-fraction acceptance tier for constrained randomization, an on-demand draw/candidate model with reproducible per-draw seeds, back-navigation that preserves configuration, GLP lockdown of redraws, and a reproduction card on the results page.

**Architecture:** The backend keeps one rejection-sampling machine (`randomizer.rs`); the acceptance rule becomes an enum (`AlphaLine` | `TopFraction`), and redraws become explicit `draw_index` values whose seeds derive deterministically from the base seed (draw 1 = base seed verbatim, so a protocol-declared seed replays with zero derivation knowledge). The frontend replaces the single-result atom with a run (candidates + selected index); the results page gains a candidate switcher and a "再抽一签" action that calls the same backend command with the next draw index.

**Tech Stack:** Rust (Tauri 2.x, rand_chacha, rust_xlsxwriter, calamine for tests), React 19 + TypeScript + Jotai, Bun.

## Global Constraints

- All Rust commands run from `src-tauri/`; frontend commands from the repo root.
- Gates for EVERY task before its commit: `cargo fmt`, `cargo test --release`, `cargo clippy --all-targets` (no new warnings), `cd .. && bun run build`. Tasks touching `core/grouping/` additionally run `cargo test --release -- --ignored` once at the end (Task 10).
- The e2e golden test (`cargo test --release --test e2e_grouping_test`) must keep passing untouched: new summary-sheet rows are written ONLY when a `RandomizationRecord` is present, so the Optimized fixture stays byte-identical. If it fails, assume the code regressed — do not regenerate the fixture.
- Code, comments, identifiers, commit messages: English. User-facing strings (`bail!` messages, UI copy, sheet labels): Chinese. Copy quoted in tasks below is verbatim from the design doc — do not paraphrase it.
- Never use `thread_rng()` on any path that must reproduce (the only allowed use stays the base-seed fallback when the user supplies none).
- Conventional commits, ≤72-char imperative subject, scopes used here: `grouping`, `export`, `frontend`, `docs`.
- Mid-branch note: after Task 1 and until Task 6 lands, the frontend still sends `enforce_criteria`, which serde ignores; `ConstrainedRandom` runs from the UI will fail backend validation in between. That is acceptable inside this branch — do not add compatibility shims.

---

### Task 1: Model the acceptance criterion as an enum (behavior-preserving)

**Files:**
- Modify: `src-tauri/src/core/models.rs` (RandomizationConfig ~line 168, RandomizationRecord ~line 292)
- Modify: `src-tauri/src/core/grouping/randomizer.rs` (compute loop ~lines 53–87, validate_randomization ~lines 161–225, record ~line 133)
- Modify: `src-tauri/src/core/exporter.rs` (`grouping_principle` ~lines 44–82)
- Modify: `src-tauri/src/core/grouping/randomizer/tests.rs` (helpers `blocked`/`plain` ~lines 50–66, plus every `enforce_criteria` mention)

**Interfaces:**
- Consumes: existing `RandomizationConfig`, `RandomizationRecord`, `compute_random_grouping`.
- Produces: `pub enum AcceptanceCriterion { AlphaLine, TopFraction { target_rate: f64 } }` (serde-tagged `type`), `RandomizationConfig.acceptance: Option<AcceptanceCriterion>`, `RandomizationRecord.acceptance: Option<AcceptanceCriterion>`. Tasks 2–9 depend on these exact names.

- [ ] **Step 1: Replace the flag with the enum in `models.rs`**

Replace the `enforce_criteria` field in `RandomizationConfig` and update `Default`; add the enum above the struct; replace `enforce_criteria` in `RandomizationRecord`:

```rust
/// Acceptance rule applied to each rejection-sampling draw. Both variants are declared
/// before any draw happens and executed by the machine, which is what keeps them inside
/// the restricted-randomization boundary; neither ranks candidates to pick a winner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AcceptanceCriterion {
    /// Every tested indicator must clear alpha. Rejects only draws with a detectable
    /// difference (~10% of them); balance is otherwise ordinary-random.
    AlphaLine,
    /// Accept only draws in the most-balanced `target_rate` fraction, ranked by min(P)
    /// over the tested indicators. The cutoff is calibrated per dataset by a seeded
    /// simulation, because min(P)'s scale collapses as the indicator count grows.
    TopFraction { target_rate: f64 },
}
```

In `RandomizationConfig`, replace:

```rust
    /// Apply the acceptance criterion to the non-primary selected indicators.
    #[serde(default)]
    pub enforce_criteria: bool,
```

with:

```rust
    /// Acceptance rule for rejection sampling. None means every draw is accepted.
    #[serde(default)]
    pub acceptance: Option<AcceptanceCriterion>,
```

Update `impl Default for RandomizationConfig` (`enforce_criteria: false` → `acceptance: None`). In `RandomizationRecord`, replace the `enforce_criteria: bool` field (and its doc comment) with:

```rust
    /// The acceptance rule that was in force. Part of the method description on export.
    pub acceptance: Option<AcceptanceCriterion>,
```

- [ ] **Step 2: Adapt `randomizer.rs`**

In `compute_random_grouping`, replace the `max_attempts` computation:

```rust
    let max_attempts = match rand_config.acceptance {
        None => 1,
        Some(_) => rand_config.max_attempts.max(1),
    };
```

Replace the loop body's criterion check:

```rust
    for attempt in 1..=max_attempts {
        let draw = plan.draw(&mut rng);

        let Some(criterion) = rand_config.acceptance else {
            accepted = Some((draw, attempt));
            break;
        };

        let score = evaluator::score_candidate(
            &draw.candidate,
            &dataset,
            &stat_config,
            Some(&group_config.sex_constraints),
            &mut scratch,
            evaluator::Untestable::Skip,
        )?;
        observed_min_p.push(score.min_p_value);

        let ok = match criterion {
            AcceptanceCriterion::AlphaLine => score.meets_criteria(stat_config.mode),
            // Rejected by validate_randomization until the calibration lands (Task 4).
            AcceptanceCriterion::TopFraction { .. } => unreachable!("rejected in validation"),
        };
        if ok {
            accepted = Some((draw, attempt));
            break;
        }
        last_rejected = Some(draw.candidate);
    }
```

In the record construction, replace `enforce_criteria: rand_config.enforce_criteria,` with `acceptance: rand_config.acceptance,`.

In `validate_randomization`: `Random` arm — `if rand_config.acceptance.is_some()` (same message); `ConstrainedRandom` arm — `if rand_config.acceptance.is_none()` (same message); the tail check becomes `if rand_config.acceptance.is_some() && rand_config.max_attempts == 0`. Add a temporary guard at the very end of the function:

```rust
    if matches!(
        rand_config.acceptance,
        Some(AcceptanceCriterion::TopFraction { .. })
    ) {
        bail!("增强档接受准则尚未启用。");
    }
```

- [ ] **Step 3: Adapt `exporter.rs`**

In `grouping_principle`, replace the `criterion_suffix` closure and its two call sites:

```rust
    let criterion_suffix = |acceptance: Option<AcceptanceCriterion>| match acceptance {
        None => String::new(),
        Some(AcceptanceCriterion::AlphaLine) => "+ 基线均衡接受准则".to_string(),
        Some(AcceptanceCriterion::TopFraction { target_rate }) => format!(
            "+ 基线均衡接受准则（仅接受最均衡的前 {:.0}%）",
            target_rate * 100.0
        ),
    };
```

Both the `Random | ConstrainedRandom` and the `BlockedRandom` arms now read the rule from the record instead of inferring it from the method name:

```rust
        GroupingMethod::Random | GroupingMethod::ConstrainedRandom => {
            let acceptance = result.randomization.as_ref().and_then(|r| r.acceptance);
            let base = if sex_stratified {
                "分层随机（分层变量：性别）"
            } else {
                "完全随机"
            };
            format!("{base}{}", criterion_suffix(acceptance))
        }
```

and in the `BlockedRandom` arm replace the `enforced` binding + `criterion_suffix(enforced)` with `criterion_suffix(result.randomization.as_ref().and_then(|r| r.acceptance))`.

- [ ] **Step 4: Update the test helpers and every remaining call site**

In `randomizer/tests.rs`:

```rust
fn blocked(seed: u64, enforce: bool) -> RandomizationConfig {
    RandomizationConfig {
        seed: Some(seed),
        primary_indicator: Some(BW.to_string()),
        acceptance: enforce.then_some(AcceptanceCriterion::AlphaLine),
        max_attempts: 1000,
    }
}

fn plain(seed: u64) -> RandomizationConfig {
    RandomizationConfig {
        seed: Some(seed),
        primary_indicator: None,
        acceptance: None,
        max_attempts: 1,
    }
}
```

Then run `rg -n "enforce_criteria" src-tauri/src src` — it must return zero Rust hits (TS hits remain until Task 6). Fix every remaining Rust site mechanically: `enforce_criteria: true` → `acceptance: Some(AcceptanceCriterion::AlphaLine)`, `enforce_criteria: false` → `acceptance: None`, assertions on `record.enforce_criteria` → `record.acceptance == Some(AcceptanceCriterion::AlphaLine)` (or `.is_none()`).

- [ ] **Step 5: Run the full backend suite**

Run (from `src-tauri/`): `cargo fmt && cargo test --release && cargo clippy --all-targets`
Expected: PASS with no new warnings — this task must not change behavior for `AlphaLine`/`None`.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "refactor(grouping): model the acceptance criterion as an enum"
```

---

### Task 2: Per-draw seed derivation (`draw_index`)

**Files:**
- Modify: `src-tauri/src/core/models.rs` (RandomizationConfig, RandomizationRecord)
- Modify: `src-tauri/src/core/grouping/randomizer.rs`
- Test: `src-tauri/src/core/grouping/randomizer/tests.rs`

**Interfaces:**
- Consumes: Task 1's `RandomizationConfig`.
- Produces: `RandomizationConfig.draw_index: usize` (serde default 1), `RandomizationRecord.base_seed: u64` + `RandomizationRecord.draw_index: usize`, `pub fn derive_draw_seed(base_seed: u64, draw_index: usize) -> u64`. Contract relied on by Tasks 3/5/9: `RandomizationRecord.seed` is the EFFECTIVE seed (replays the allocation at `draw_index = 1`); for draw 1, `seed == base_seed`.

- [ ] **Step 1: Write the failing tests**

Append to `randomizer/tests.rs` (the helpers `plain`, `group_config`, `female_constraints`, `dataset_60f`, `stat_config`, `run`, `allocation`, `replay` already exist there):

```rust
#[test]
fn draw_one_uses_the_base_seed_verbatim() {
    let config = group_config(
        female_constraints(3, 20),
        GroupingMethod::Random,
        plain(42),
    );
    let record = run(dataset_60f(), config, stat_config(&[BW, CD45]))
        .randomization
        .unwrap();

    assert_eq!(record.seed, 42);
    assert_eq!(record.base_seed, 42);
    assert_eq!(record.draw_index, 1);
}

#[test]
fn later_draws_are_reproducible_distinct_and_replayable_from_their_seed() {
    let make = |k: usize| {
        let mut config = plain(42);
        config.draw_index = k;
        group_config(female_constraints(3, 20), GroupingMethod::Random, config)
    };
    let stats = || stat_config(&[BW, CD45]);

    let draw2a = run(dataset_60f(), make(2), stats());
    let draw2b = run(dataset_60f(), make(2), stats());
    let draw3 = run(dataset_60f(), make(3), stats());
    let draw1 = run(dataset_60f(), make(1), stats());

    assert_eq!(allocation(&draw2a), allocation(&draw2b));
    assert_ne!(allocation(&draw2a), allocation(&draw3));
    assert_ne!(allocation(&draw1), allocation(&draw2a));

    let record = draw2a.randomization.clone().unwrap();
    assert_eq!(record.base_seed, 42);
    assert_eq!(record.draw_index, 2);
    assert_ne!(record.seed, 42, "the effective seed must be the derived one");

    // The recorded effective seed alone must replay the allocation — this is the
    // reproduction contract QA relies on, with no knowledge of the derivation.
    let replayed = run(
        dataset_60f(),
        group_config(
            female_constraints(3, 20),
            GroupingMethod::Random,
            plain(record.seed),
        ),
        stats(),
    );
    assert_eq!(allocation(&draw2a), allocation(&replayed));
}

#[test]
fn the_exported_draw_reproduces_a_later_draw() {
    let mut config = plain(42);
    config.draw_index = 3;
    let result = run(
        dataset_60f(),
        group_config(female_constraints(3, 20), GroupingMethod::Random, config),
        stat_config(&[BW, CD45]),
    );
    assert_eq!(replay(&result, &[20, 20, 20], 1), allocation(&result));
}

#[test]
fn draw_index_zero_is_rejected() {
    let mut config = plain(42);
    config.draw_index = 0;
    let err = compute_random_grouping(
        dataset_60f(),
        group_config(female_constraints(3, 20), GroupingMethod::Random, config),
        stat_config(&[BW, CD45]),
    )
    .unwrap_err()
    .to_string();
    assert!(err.contains("抽签序号"), "{err}");
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --release draw_ -- --nocapture` and `cargo test --release later_draws`
Expected: FAIL — compile errors (`draw_index`, `base_seed` fields do not exist yet).

- [ ] **Step 3: Implement**

`models.rs` — add to `RandomizationConfig` (and `draw_index: 1` to `Default` and to the two test helpers `plain`/`blocked`):

```rust
    /// 1-based draw number within a run. Draw 1 uses the base seed verbatim, so a
    /// protocol-declared seed replays the allocation with no derivation knowledge;
    /// later draws (exploratory only) derive their seed from (base_seed, draw_index).
    #[serde(default = "default_draw_index")]
    pub draw_index: usize,
```

```rust
fn default_draw_index() -> usize {
    1
}
```

Add to `RandomizationRecord`, directly after `seed`:

```rust
    /// The seed the user supplied (or the backend generated). Equal to `seed` at draw 1.
    pub base_seed: u64,
    /// Which draw of the run this is.
    pub draw_index: usize,
```

`randomizer.rs` — add near `gcd`:

```rust
/// SplitMix64 finalizer, used to derive per-draw seeds from the base seed.
fn splitmix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Draw 1 is the base seed itself: a GLP protocol pins its allocation with the seed it
/// declared. Later draws mix the index in, so every draw stays pinned by (base, k).
pub fn derive_draw_seed(base_seed: u64, draw_index: usize) -> u64 {
    if draw_index <= 1 {
        base_seed
    } else {
        splitmix64(
            base_seed.wrapping_add((draw_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)),
        )
    }
}
```

In `compute_random_grouping`, replace the seed block:

```rust
    let base_seed = rand_config
        .seed
        .unwrap_or_else(|| rand::thread_rng().gen::<u64>());
    let seed = derive_draw_seed(base_seed, rand_config.draw_index);
    let mut rng = ChaCha12Rng::seed_from_u64(seed);
```

Record gains `base_seed,` and `draw_index: rand_config.draw_index,`. In `validate_randomization`, before the `max_attempts` check:

```rust
    if rand_config.draw_index == 0 {
        bail!("抽签序号从 1 开始。");
    }
```

- [ ] **Step 4: Run the tests, then the whole suite**

Run: `cargo test --release randomizer` then `cargo fmt && cargo test --release && cargo clippy --all-targets`
Expected: all PASS (existing reproducibility tests prove draw-1 behavior is unchanged).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(grouping): derive per-draw seeds so redraws are reproducible"
```

---

### Task 3: GLP scenario rejects redraws (backend defense)

**Files:**
- Modify: `src-tauri/src/core/grouping/randomizer.rs` (`validate_randomization`)
- Test: `src-tauri/src/core/grouping/randomizer/tests.rs`

**Interfaces:**
- Consumes: Task 2's `draw_index`.
- Produces: backend guarantee the UI gating (Task 9) cannot be bypassed over IPC.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn glp_scenario_rejects_redraws() {
    let mut rand_config = plain(42);
    rand_config.draw_index = 2;
    let mut config = group_config(
        female_constraints(3, 20),
        GroupingMethod::Random,
        rand_config,
    );
    config.scenario = StudyScenario::GlpSubmission;

    let err = compute_random_grouping(dataset_60f(), config, stat_config(&[BW, CD45]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("分配隐藏"), "{err}");
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --release glp_scenario_rejects_redraws`
Expected: FAIL — the run succeeds instead of erroring.

- [ ] **Step 3: Implement**

In `validate_randomization`, right after the `draw_index == 0` check:

```rust
    // The UI greys the redraw controls out under GLP, but a greyed-out button does not
    // stop a hand-built IPC request; allocation concealment is enforced here too.
    if group_config.scenario == StudyScenario::GlpSubmission && rand_config.draw_index > 1 {
        bail!(
            "GLP 场景执行分配隐藏：一次抽签即为最终分配，不提供重抽入口（抽签序号必须为 1）。\
             需要更高的均衡度，请在计算前调整接受准则。"
        );
    }
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --release randomizer && cargo fmt && cargo clippy --all-targets`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(grouping): reject redraws under the GLP scenario"
```

---

### Task 4: Calibrated `TopFraction` acceptance

**Files:**
- Modify: `src-tauri/src/core/models.rs` (RandomizationRecord)
- Modify: `src-tauri/src/core/grouping/randomizer.rs`
- Test: `src-tauri/src/core/grouping/randomizer/tests.rs`

**Interfaces:**
- Consumes: Task 1's enum, Task 2's `splitmix64`.
- Produces: `RandomizationRecord.calibrated_threshold: Option<f64>` and `RandomizationRecord.calibration_draws: Option<usize>` (Task 5 exports them, Task 9 displays them); working `TopFraction` runs.

- [ ] **Step 1: Write the failing tests**

Add a helper and three tests to `randomizer/tests.rs`:

```rust
fn constrained_top(seed: u64, target_rate: f64) -> RandomizationConfig {
    RandomizationConfig {
        seed: Some(seed),
        primary_indicator: None,
        acceptance: Some(AcceptanceCriterion::TopFraction { target_rate }),
        max_attempts: 10_000,
        draw_index: 1,
    }
}

#[test]
fn top_fraction_accepts_only_above_the_calibrated_threshold() {
    let result = run(
        dataset_60f(),
        group_config(
            female_constraints(3, 20),
            GroupingMethod::ConstrainedRandom,
            constrained_top(42, 0.10),
        ),
        stat_config(&[BW, CD45]),
    );

    let record = result.randomization.clone().unwrap();
    let threshold = record
        .calibrated_threshold
        .expect("top-fraction runs must record their threshold");
    assert_eq!(record.calibration_draws, Some(1000));

    let min_p = result
        .statistics
        .iter()
        .map(|s| s.diff_p_value)
        .fold(f64::INFINITY, f64::min);
    assert!(
        min_p >= threshold,
        "accepted draw min_p {min_p} must clear the threshold {threshold}"
    );
    // On two indicators the top-10% cut sits far above the alpha line (design doc §4.3:
    // random min(P) has q90 ~ 0.69); a threshold near alpha would mean the calibration
    // quantile is wired backwards.
    assert!(threshold > 0.3, "threshold {threshold} is implausibly low");
}

#[test]
fn top_fraction_calibration_and_allocation_are_reproducible() {
    let make = || {
        group_config(
            female_constraints(3, 20),
            GroupingMethod::ConstrainedRandom,
            constrained_top(42, 0.10),
        )
    };
    let a = run(dataset_60f(), make(), stat_config(&[BW, CD45]));
    let b = run(dataset_60f(), make(), stat_config(&[BW, CD45]));

    let (ra, rb) = (a.randomization.clone().unwrap(), b.randomization.clone().unwrap());
    assert_eq!(ra.calibrated_threshold, rb.calibrated_threshold);
    assert_eq!(ra.attempts, rb.attempts);
    assert_eq!(allocation(&a), allocation(&b));
}

#[test]
fn top_fraction_rejects_an_out_of_range_rate() {
    for rate in [0.0, -0.2, 1.5] {
        let err = compute_random_grouping(
            dataset_60f(),
            group_config(
                female_constraints(3, 20),
                GroupingMethod::ConstrainedRandom,
                constrained_top(42, rate),
            ),
            stat_config(&[BW, CD45]),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("目标接受率"), "rate {rate}: {err}");
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test --release top_fraction`
Expected: FAIL — compile error (`calibrated_threshold` missing), then the temporary "尚未启用" guard.

- [ ] **Step 3: Implement**

`models.rs` — append to `RandomizationRecord`:

```rust
    /// Present only for `TopFraction`: the calibrated min(P) cutoff a draw had to clear.
    pub calibrated_threshold: Option<f64>,
    /// How many seeded simulation draws produced the cutoff.
    pub calibration_draws: Option<usize>,
```

`randomizer.rs` — constants near `RNG_ALGORITHM`:

```rust
/// Tag mixed into the calibration RNG seed so calibration and the formal draws consume
/// distinct, individually reproducible streams.
const CALIBRATION_TAG: u64 = 0x4143_4345_5054_0000; // "ACCEPT\0\0"
const CALIBRATION_DRAWS: usize = 1000;
```

Calibration function (after `build_plan`):

```rust
/// Fix the min(P) cutoff for `TopFraction` on this dataset. A fixed threshold cannot
/// work: min(P)'s scale collapses as the indicator count grows (median ~0.30 at 2
/// indicators, ~0.01 at 70), so the rule the user declares is a target acceptance rate
/// and the cutoff is its empirical quantile under seeded simulation.
fn calibrate_threshold(
    plan: &Plan,
    dataset: &Dataset,
    stat_config: &StatConfig,
    sex_constraints: &[SexConstraint],
    seed: u64,
    target_rate: f64,
) -> Result<f64> {
    let mut rng = ChaCha12Rng::seed_from_u64(splitmix64(seed ^ CALIBRATION_TAG));
    let mut scratch = evaluator::EvalScratch::default();
    let mut min_ps: Vec<f64> = Vec::with_capacity(CALIBRATION_DRAWS);

    for _ in 0..CALIBRATION_DRAWS {
        let draw = plan.draw(&mut rng);
        let score = evaluator::score_candidate(
            &draw.candidate,
            dataset,
            stat_config,
            Some(sex_constraints),
            &mut scratch,
            evaluator::Untestable::Skip,
        )?;
        if score.min_p_value.is_finite() {
            min_ps.push(score.min_p_value);
        }
    }

    if min_ps.is_empty() {
        bail!("定标失败：模拟抽样没有任何可检验的指标，无法确定接受门槛。请检查所选指标。");
    }

    min_ps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let idx = ((min_ps.len() - 1) as f64 * (1.0 - target_rate)).round() as usize;
    Ok(min_ps[idx])
}
```

In `compute_random_grouping`, after `build_plan` and before the attempts loop:

```rust
    let threshold = match rand_config.acceptance {
        Some(AcceptanceCriterion::TopFraction { target_rate }) => Some(calibrate_threshold(
            &plan,
            &dataset,
            &stat_config,
            &group_config.sex_constraints,
            seed,
            target_rate,
        )?),
        _ => None,
    };

    let max_attempts = match rand_config.acceptance {
        None => 1,
        Some(AcceptanceCriterion::AlphaLine) => rand_config.max_attempts.max(1),
        // Expected draws ~ 1/target_rate; 50x headroom keeps unlucky streaks from
        // failing a run that would succeed a moment later.
        Some(AcceptanceCriterion::TopFraction { target_rate }) => rand_config
            .max_attempts
            .max((50.0 / target_rate).ceil() as usize),
    };
```

Loop check — replace the `unreachable!` arm:

```rust
        let ok = match criterion {
            AcceptanceCriterion::AlphaLine => score.meets_criteria(stat_config.mode),
            AcceptanceCriterion::TopFraction { .. } => {
                score.min_p_value >= threshold.expect("calibrated before the loop")
            }
        };
```

Record gains `calibrated_threshold: threshold,` and `calibration_draws: threshold.map(|_| CALIBRATION_DRAWS),`.

`validate_randomization` — replace the temporary "尚未启用" guard with the range check:

```rust
    if let Some(AcceptanceCriterion::TopFraction { target_rate }) = rand_config.acceptance {
        if !(target_rate > 0.0 && target_rate <= 1.0) {
            bail!("目标接受率必须在 (0, 1] 区间内。");
        }
    }
```

`acceptance_failure` — make the message criterion-aware. Change the signature to take `criterion_desc: &str` instead of computing from alpha, build the description at the call site:

```rust
        None => {
            let criterion_desc = match (rand_config.acceptance, threshold) {
                (Some(AcceptanceCriterion::TopFraction { target_rate }), Some(p0)) => format!(
                    "仅接受最均衡的前 {:.0}%，即 min(P) ≥ {:.4}",
                    target_rate * 100.0,
                    p0
                ),
                _ => format!("全部指标 P > {}", stat_config.alpha),
            };
            return Err(acceptance_failure(
                &dataset,
                &group_config,
                &stat_config,
                last_rejected.as_ref(),
                &observed_min_p,
                max_attempts,
                &criterion_desc,
            ));
        }
```

and in `acceptance_failure` change the first line of the message from `"抽样 {} 次仍未满足接受准则（α = {}）。\n..."` to `"抽样 {} 次仍未满足接受准则（{criterion_desc}）。\n..."` (drop the `stat_config.alpha` format argument; keep everything else, including the closing "请勿反复更换种子重算" line).

- [ ] **Step 4: Run the tests, then the whole suite**

Run: `cargo test --release top_fraction` then `cargo fmt && cargo test --release && cargo clippy --all-targets`
Expected: PASS. The calibration adds 1000 candidate scorings (~the cost the optimizer pays for 1% of one run) — if `top_fraction` tests take more than a few seconds each, something is wrong.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(grouping): calibrated top-fraction acceptance criterion"
```

---

### Task 5: Export acceptance and draw provenance in the summary sheet

**Files:**
- Modify: `src-tauri/src/core/exporter.rs` (summary sheet, after the 抽样次数 row ~line 730)
- Test: `src-tauri/src/core/exporter_test.rs`

**Interfaces:**
- Consumes: `RandomizationRecord.{acceptance, base_seed, draw_index, calibrated_threshold, calibration_draws}` from Tasks 1/2/4.
- Produces: three new 汇总信息 rows — labels `接受准则`, `主种子`, `抽签序号` — written ONLY when a record is present (the Optimized golden fixture must stay untouched).

- [ ] **Step 1: Write the failing test**

Append to the `export_integration_tests` module in `exporter_test.rs` (helpers `fixture_path` and `read_sheet` already exist there; complete the `SheetConfig` literal with the same remaining fields `export_isolates_the_reserve_group` uses):

```rust
    #[test]
    fn summary_sheet_records_the_acceptance_and_draw_provenance() {
        let dataset = parser::parse_excel_file(&fixture_path(
            "tests/fixtures/randomization_input_60f.xlsx",
        ))
        .expect("fixture must parse");

        let constraints: Vec<SexConstraint> = (0..3)
            .map(|i| SexConstraint {
                group_index: i,
                male_count: 0,
                female_count: 20,
                group_type: GroupType::Experimental,
                custom_name: None,
            })
            .collect();

        let indicators = vec!["体重".to_string(), "CD45 比例".to_string()];

        let group_config = GroupConfig {
            num_groups: 3,
            animals_per_group: GroupSize::Uniform { value: 20 },
            sex_constraints: constraints.clone(),
            scenario: StudyScenario::Exploratory,
            method: GroupingMethod::ConstrainedRandom,
            randomization: Some(RandomizationConfig {
                seed: Some(42),
                primary_indicator: None,
                acceptance: Some(AcceptanceCriterion::TopFraction { target_rate: 0.10 }),
                max_attempts: 10_000,
                draw_index: 2,
            }),
        };

        let stat_config = StatConfig {
            selected_indicators: indicators.clone(),
            alpha: 0.05,
            mode: OptimizationMode::Strict,
            max_candidates: 1,
        };

        let result = grouping::compute_grouping(dataset.clone(), group_config, stat_config)
            .expect("randomized run must succeed")
            .candidates
            .remove(0);
        let record = result.randomization.clone().expect("record must be present");

        let output_dir = std::env::temp_dir().join("autogroup_export_acceptance");
        std::fs::create_dir_all(&output_dir).expect("temp dir");
        let output = output_dir
            .join("acceptance.xlsx")
            .to_str()
            .unwrap()
            .to_string();

        let sheet_config = exporter::SheetConfig {
            scenario: StudyScenario::Exploratory,
            selected_indicators: indicators,
            include_statistics: true,
            include_summary: true,
            group_constraints: Some(constraints),
        };
        exporter::export_grouping_result(&result, &dataset, &sheet_config, &output)
            .expect("export must succeed");

        let rows = read_sheet(&output, "汇总信息");
        let value_of = |label: &str| -> String {
            rows.iter()
                .find(|row| row.first().map(String::as_str) == Some(label))
                .unwrap_or_else(|| panic!("summary sheet must contain a {label} row"))
                .get(1)
                .cloned()
                .unwrap_or_default()
        };

        assert!(value_of("接受准则").contains("仅接受最均衡的前 10%"));
        assert!(value_of("接受准则").contains("定标抽样 1000 次"));
        assert_eq!(value_of("主种子"), "42");
        assert_eq!(value_of("抽签序号"), "2");
        assert_eq!(value_of("随机种子"), record.seed.to_string());
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test --release summary_sheet_records`
Expected: FAIL — "summary sheet must contain a 接受准则 row".

- [ ] **Step 3: Implement**

In the summary sheet writer, immediately after the 抽样次数 row (`row += 1;` at ~line 730) and before the 输入指纹 row, add — inside a `record` presence check so a non-randomized export is byte-identical to before:

```rust
    if let Some(r) = record {
        sheet.write_string(row, 0, "接受准则")?;
        let acceptance_text = match r.acceptance {
            None => "无（纯随机）".to_string(),
            Some(AcceptanceCriterion::AlphaLine) => "全部所选指标 P > α".to_string(),
            Some(AcceptanceCriterion::TopFraction { target_rate }) => format!(
                "仅接受最均衡的前 {:.0}%（min(P) ≥ {}，定标抽样 {} 次）",
                target_rate * 100.0,
                r.calibrated_threshold
                    .map_or("未记录".to_string(), |t| format!("{t:.6}")),
                r.calibration_draws.unwrap_or(0),
            ),
        };
        sheet.write_string(row, 1, &acceptance_text)?;
        row += 1;

        // Seeds are written as strings everywhere in this sheet: u64 exceeds f64's
        // integer precision and Excel would silently round it.
        sheet.write_string(row, 0, "主种子")?;
        sheet.write_string(row, 1, r.base_seed.to_string())?;
        row += 1;

        sheet.write_string(row, 0, "抽签序号")?;
        sheet.write_number(row, 1, r.draw_index as f64)?;
        row += 1;
    }
```

- [ ] **Step 4: Run the tests — including the untouched golden fixture**

Run: `cargo test --release summary_sheet_records && cargo test --release --test e2e_grouping_test && cargo fmt && cargo test --release && cargo clippy --all-targets`
Expected: PASS everywhere. If e2e fails on 汇总信息, the new rows leaked into the non-randomized path — fix the guard, do NOT regenerate the fixture.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(export): record acceptance criterion and draw provenance"
```

---

### Task 6: Frontend types, tier copy, and the ConfigurePage tier selector

**Files:**
- Modify: `src/types/index.ts` (RandomizationConfig ~line 66, RandomizationRecord ~line 139)
- Modify: `src/lib/grouping-method.ts`
- Modify: `src/components/features/ConfigurePage.tsx` (state ~line 53, handleNext ~lines 262–280, and the JSX that renders the `enforceCriteria` checkbox)

**Interfaces:**
- Consumes: backend serde shapes from Tasks 1/2/4.
- Produces: TS `AcceptanceCriterion`, updated `RandomizationConfig`/`RandomizationRecord`, `ACCEPTANCE_TIERS` / `ACCEPTANCE_FOOTNOTE` / `TARGET_RATE_PRESETS` exports, ConfigurePage state `acceptanceTier` / `targetRate` / `criterionOn` (Task 8 hydrates these names).

- [ ] **Step 1: Mirror the types**

`src/types/index.ts` — add above `RandomizationConfig`:

```ts
/**
 * Pre-declared acceptance rule for rejection sampling. Mirrors the Rust enum's
 * internally-tagged serde shape.
 */
export type AcceptanceCriterion =
  | { type: "AlphaLine" }
  | { type: "TopFraction"; target_rate: number };
```

In `RandomizationConfig`, replace `enforce_criteria: boolean;` with:

```ts
  /** Null means every draw is accepted (pure randomization). */
  acceptance: AcceptanceCriterion | null;
  /** 1-based draw number within a run. Always 1 when computed from the configure page. */
  draw_index: number;
```

In `RandomizationRecord`, replace `enforce_criteria: boolean;` with:

```ts
  acceptance: AcceptanceCriterion | null;
  /** The seed the user supplied or the backend generated. Equal to `seed` at draw 1. */
  base_seed: number;
  draw_index: number;
  /** Present only for TopFraction: the calibrated min(P) cutoff. */
  calibrated_threshold?: number | null;
  calibration_draws?: number | null;
```

- [ ] **Step 2: Add the tier copy to `grouping-method.ts`**

Copy is verbatim from the design doc §2.3 — do not edit it:

```ts
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
```

If `MethodCopy.enforceCriteria` is referenced anywhere, rename the field to `acceptance: "none" | "required" | "optional"` (`Random` → `"none"`, `ConstrainedRandom` → `"required"`, `BlockedRandom` → `"optional"`, `Optimized`/`Minimization` → `"none"`) and update the reference sites; if it is unreferenced outside the array literal, delete it.

- [ ] **Step 3: Rework ConfigurePage state and submission**

Replace `const [enforceCriteria, setEnforceCriteria] = useState(true);` with:

```ts
  const [acceptanceTier, setAcceptanceTier] = useState<"alpha" | "topfraction">("alpha");
  const [targetRate, setTargetRate] = useState(0.1);
  /** BlockedRandom only: whether the (optional) criterion is on at all. */
  const [criterionOn, setCriterionOn] = useState(true);
```

In `handleNext`, replace the `criterionOn`/`enforce_criteria` block with:

```ts
    const wantsCriterion =
      method === "ConstrainedRandom" || (method === "BlockedRandom" && criterionOn);
    const acceptance: AcceptanceCriterion | null =
      isRandomized && wantsCriterion
        ? acceptanceTier === "topfraction"
          ? { type: "TopFraction", target_rate: targetRate }
          : { type: "AlphaLine" }
        : null;

    const parsedSeed = seedText.trim() === "" ? null : Number(seedText.trim());
    const randomization: RandomizationConfig | null = isRandomized
      ? {
          seed: parsedSeed !== null && Number.isFinite(parsedSeed) ? parsedSeed : null,
          primary_indicator: method === "BlockedRandom" ? primaryIndicator : null,
          acceptance,
          max_attempts: 10000,
          draw_index: 1,
        }
      : null;
```

(keep any existing fields of the literal that already match; update the `useCallback` dependency array: drop `enforceCriteria`, add `acceptanceTier`, `targetRate`, `criterionOn`). Import `AcceptanceCriterion` from `@/types` and `ACCEPTANCE_TIERS, ACCEPTANCE_FOOTNOTE, TARGET_RATE_PRESETS` from `@/lib/grouping-method`.

- [ ] **Step 4: Replace the criterion checkbox JSX with the tier selector**

Where the `enforceCriteria` checkbox currently renders (search the JSX for it), render instead:

```tsx
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
            <p className="mt-1 text-xs text-muted-foreground">{tier.description}</p>
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
```

- [ ] **Step 5: Build and spot-check**

Run (repo root): `bun run build`, then `rg -n "enforce_criteria" src/` — zero hits.
Expected: clean tsc + vite build.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(frontend): acceptance tier selection with pre-declared copy"
```

---

### Task 7: Hold the full candidate run in state

**Files:**
- Modify: `src/stores/index.ts`
- Modify: `src/components/features/ComputePage.tsx` (result handling ~lines 85–109)

**Interfaces:**
- Consumes: existing `MultiGroupingResult`.
- Produces: `GroupingRun` interface, `groupingRunAtom` (read/write), `resultAtom` becomes a READ-ONLY derived atom over the run. Tasks 8/9 use exactly `groupingRunAtom` and `GroupingRun { candidates, selectedIndex, totalEvaluated, totalValid }`.

- [ ] **Step 1: Rework the store**

In `src/stores/index.ts`, replace `export const resultAtom = atom<GroupingResult | null>(null);` with:

```ts
/** One computation run: every candidate it produced and which one is on display. */
export interface GroupingRun {
  candidates: GroupingResult[];
  selectedIndex: number;
  totalEvaluated: number;
  totalValid: number;
}

export const groupingRunAtom = atom<GroupingRun | null>(null);

/** The candidate currently on display; a read-only view over the run. */
export const resultAtom = atom<GroupingResult | null>((get) => {
  const run = get(groupingRunAtom);
  return run ? (run.candidates[run.selectedIndex] ?? null) : null;
});
```

Update `hasResultAtom` to derive from `groupingRunAtom`, and in `resetStateAtom` replace `set(resultAtom, null)` with `set(groupingRunAtom, null)`. Keep the `GroupingResult` type import.

- [ ] **Step 2: Store the whole run in ComputePage**

Replace `const [, setResult] = useAtom(resultAtom);` with `const [, setRun] = useAtom(groupingRunAtom);` (adjust the import), and replace the best-candidate selection block (keep the existing sort):

```ts
        if (multiResult.candidates && multiResult.candidates.length > 0) {
          const sortedCandidates = [...multiResult.candidates].sort((a, b) => {
            const minPDiff = b.summary.min_p_value - a.summary.min_p_value;
            if (Math.abs(minPDiff) > 1e-10) {
              return minPDiff;
            }
            return b.summary.mean_p_value - a.summary.mean_p_value;
          });

          setRun({
            candidates: sortedCandidates,
            selectedIndex: 0,
            totalEvaluated: multiResult.total_evaluated,
            totalValid: multiResult.total_valid,
          });
          setStatus("success");

          setTimeout(() => {
            setCurrentStep("results");
          }, 1500);
        } else {
          throw new Error("No valid grouping solution found");
        }
```

Update the effect dependency array (`setResult` → `setRun`).

- [ ] **Step 3: Build**

Run: `bun run build`
Expected: PASS — `ResultsPage` keeps reading `resultAtom`, which still yields a `GroupingResult | null`.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(frontend): hold the full candidate run in state"
```

---

### Task 8: Back navigation with configuration preserved

**Files:**
- Modify: `src/components/features/ResultsPage.tsx` (action buttons ~line 437)
- Modify: `src/components/features/ConfigurePage.tsx` (state initializers ~lines 42–63, constraints effect ~lines 130–155, page header JSX)

**Interfaces:**
- Consumes: `groupingRunAtom` (Task 7), `groupConfigAtom` / `statConfigAtom` (existing), ConfigurePage state names from Task 6.
- Produces: "返回修改配置" on the results page; ConfigurePage hydrates every control from the stored config on mount; a stale-run banner.

- [ ] **Step 1: Add the back button to ResultsPage**

Import `currentStepAtom` from `@/stores`, add `const [, setCurrentStep] = useAtom(currentStepAtom);`, and in the action-buttons row add between 重新开始 and 导出结果:

```tsx
        <Button variant="outline" onClick={() => setCurrentStep("configure")}>
          返回修改配置
        </Button>
```

- [ ] **Step 2: Hydrate ConfigurePage from the stored config**

Read the stored atoms and derive the stored constraint split (place right after the existing atom hooks):

```ts
  const [storedGroupConfig] = useAtom(groupConfigAtom);
  const [storedStatConfig] = useAtom(statConfigAtom);
  const storedExperimental = useMemo(
    () =>
      storedGroupConfig?.sex_constraints.filter((c) => c.group_type !== "Reserve") ?? [],
    [storedGroupConfig]
  );
  const storedReserve = storedGroupConfig?.sex_constraints.find(
    (c) => c.group_type === "Reserve"
  );
```

(The page currently only writes `groupConfigAtom` via `setGroupConfig`; change that hook to `useAtom(groupConfigAtom)` destructured as `[storedGroupConfig, setGroupConfig]`, same for `statConfigAtom`.)

Convert every form-state `useState` initial value to a lazy initializer from the stored config:

```ts
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
  const [scenario, setScenario] = useState<StudyScenario>(
    () => storedGroupConfig?.scenario ?? "Exploratory"
  );
  const [method, setMethod] = useState<GroupingMethod>(
    () => storedGroupConfig?.method ?? defaultMethodFor("Exploratory")
  );
  const [primaryIndicator, setPrimaryIndicator] = useState<string>(
    () => storedGroupConfig?.randomization?.primary_indicator ?? ""
  );
  const [seedText, setSeedText] = useState<string>(
    () => storedGroupConfig?.randomization?.seed?.toString() ?? ""
  );
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
  const [criterionOn, setCriterionOn] = useState(() =>
    storedGroupConfig?.method === "BlockedRandom"
      ? storedGroupConfig.randomization?.acceptance != null
      : true
  );
  const [reserveMaleCount, setReserveMaleCount] = useState(
    () => storedReserve?.male_count ?? 0
  );
  const [reserveFemaleCount, setReserveFemaleCount] = useState(
    () => storedReserve?.female_count ?? 0
  );
```

The `scenarioInitialized` ref already skips the method-reset effect on first render, so the hydrated method survives mount.

- [ ] **Step 3: Preserve per-group counts through the constraints-init effect**

The effect at ~line 130 rebuilds `sexConstraints` with an even split on every mount, which would discard hand-edited quotas. Guard its first run:

```ts
  const constraintsHydrated = useRef(false);
  useEffect(() => {
    if (!dataset) return;

    if (!constraintsHydrated.current) {
      constraintsHydrated.current = true;
      if (storedExperimental.length > 0 && storedExperimental.length === numGroups) {
        setSexConstraints(storedExperimental);
        return;
      }
    }

    // ... existing even-distribution body, unchanged ...
  }, [numGroups, dataset, reserveMaleCount, reserveFemaleCount, storedExperimental]);
```

- [ ] **Step 4: Add the stale-run banner**

Import `groupingRunAtom`, read it (`const [existingRun] = useAtom(groupingRunAtom);`), and render at the top of the page content:

```tsx
      {existingRun && (
        <Alert>
          <AlertDescription>
            当前已有计算结果。修改配置并重新计算后，将开始新的一次运行，现有结果与其全部候选将被替换。
          </AlertDescription>
        </Alert>
      )}
```

- [ ] **Step 5: Build and manually verify the round trip**

Run: `bun run build`, then `bun run tauri dev`: import the fixture `src-tauri/tests/fixtures/randomization_input_60f.xlsx`, configure a 3×20 `ConstrainedRandom` run with seed 42, compute, then 返回修改配置 — every control (scenario, method, seed, tier, rate, group counts) must show what was submitted, with the stale-run banner visible.
Expected: settings preserved; changing nothing and recomputing reproduces the same allocation (same seed, draw 1).

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(frontend): return to configuration with settings preserved"
```

---

### Task 9: Candidate switcher, redraws, GLP gating, reproduction card

**Files:**
- Modify: `src/components/features/ResultsPage.tsx`

**Interfaces:**
- Consumes: `groupingRunAtom`/`GroupingRun` (Task 7), `RandomizationRecord.{base_seed, draw_index, acceptance, calibrated_threshold}` (Task 6 types), backend redraw contract (same command, `draw_index = k`, seed = `base_seed`) from Task 2, backend GLP rejection from Task 3.
- Produces: the complete results-page interaction surface. GLP copy below is verbatim from the design doc §4.1 — do not paraphrase.

- [ ] **Step 1: Wire the run atom and the redraw action**

Add imports (`useState` from react, `MultiGroupingResult` type, `groupingRunAtom` from stores). Replace `const [result] = useAtom(resultAtom);` usage by reading both atoms:

```ts
  const [run, setRun] = useAtom(groupingRunAtom);
  const [result] = useAtom(resultAtom);
  const [redrawing, setRedrawing] = useState(false);

  const isGlp = groupConfig?.scenario === "GlpSubmission";
```

After the early returns (so `result` is non-null), add:

```ts
  const isRandomizedRun = result.method !== "Optimized";

  const handleSelectCandidate = (index: number) => {
    if (!run || isGlp) return;
    setRun({ ...run, selectedIndex: index });
  };

  const handleRedraw = async () => {
    if (!run || !dataset || !statConfig || !groupConfig?.randomization || isGlp) return;
    const lastRecord = run.candidates[run.candidates.length - 1]?.randomization;
    if (!lastRecord) return;
    const nextIndex =
      Math.max(...run.candidates.map((c) => c.randomization?.draw_index ?? 1)) + 1;

    setRedrawing(true);
    try {
      const multi = await invoke<MultiGroupingResult>("compute_grouping", {
        dataset,
        groupConfig: {
          ...groupConfig,
          randomization: {
            ...groupConfig.randomization,
            // The base seed pins the whole draw sequence; the index picks the draw.
            seed: lastRecord.base_seed,
            draw_index: nextIndex,
          },
        },
        statConfig,
      });
      const drawn = multi.candidates[0];
      if (drawn) {
        setRun({
          ...run,
          candidates: [...run.candidates, drawn],
          selectedIndex: run.candidates.length,
        });
      }
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setRedrawing(false);
    }
  };
```

- [ ] **Step 2: Render the candidate switcher card**

Insert as the first card after the summary-cards grid:

```tsx
      {run && (isRandomizedRun || run.candidates.length > 1) && (
        <Card>
          <CardHeader>
            <CardTitle>候选分组</CardTitle>
            <CardDescription>
              {isRandomizedRun
                ? "每一签都由（主种子，抽签序号）唯一决定，可随时复现；抽过的签全部保留"
                : "优化模式返回的 Top-N 排名，按 min(P) 与 mean(P) 降序"}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            <div className="flex flex-wrap gap-2">
              {run.candidates.map((candidate, index) => (
                <Button
                  key={index}
                  size="sm"
                  variant={index === run.selectedIndex ? "default" : "outline"}
                  disabled={isGlp}
                  onClick={() => handleSelectCandidate(index)}
                >
                  {isRandomizedRun
                    ? `第 ${candidate.randomization?.draw_index ?? index + 1} 签`
                    : `排名 #${index + 1} · min(P)=${candidate.summary.min_p_value.toFixed(4)}`}
                </Button>
              ))}
              {isRandomizedRun && (
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={isGlp || redrawing}
                  onClick={handleRedraw}
                >
                  {redrawing ? "抽签中…" : "再抽一签"}
                </Button>
              )}
            </div>
            {isGlp && (
              <Alert>
                <AlertDescription className="text-sm">
                  GLP 场景执行分配隐藏：一次抽签即为最终分配，不提供看到结果后重抽或挑选的入口。
                  需要更高的均衡度，请在计算前调整接受准则的目标接受率。
                </AlertDescription>
              </Alert>
            )}
            <p className="text-xs text-muted-foreground">
              导出将使用当前选中的候选（
              {isRandomizedRun
                ? `第 ${result.randomization?.draw_index ?? run.selectedIndex + 1} 签`
                : `排名 #${run.selectedIndex + 1}`}
              ）。
            </p>
          </CardContent>
        </Card>
      )}
```

- [ ] **Step 3: Extend the traceability card into the reproduction card**

Inside the existing `{record && (<>...</>)}` grid block, add three entries after 引擎版本:

```tsx
                <div>
                  <div className="text-muted-foreground text-xs">抽签序号</div>
                  <div className="font-medium">第 {record.draw_index} 签</div>
                </div>
                {record.draw_index > 1 && (
                  <div>
                    <div className="text-muted-foreground text-xs">主种子</div>
                    <div className="font-medium font-mono">{record.base_seed}</div>
                  </div>
                )}
                <div>
                  <div className="text-muted-foreground text-xs">接受准则</div>
                  <div className="font-medium">
                    {record.acceptance == null
                      ? "无（纯随机）"
                      : record.acceptance.type === "AlphaLine"
                        ? "全部所选指标 P > α"
                        : `仅接受最均衡的前 ${Math.round(record.acceptance.target_rate * 100)}%（min(P) ≥ ${record.calibrated_threshold?.toFixed(4) ?? "—"}）`}
                  </div>
                </div>
```

After the grid (still inside the card, before the primary-indicator alert), add the reproduction steps:

```tsx
          {record && (
            <div className="rounded-md bg-muted/50 p-3 text-sm space-y-1">
              <div className="font-medium">复现步骤</div>
              <ol className="list-decimal list-inside text-muted-foreground space-y-0.5">
                <li>导入同一份数据文件（软件校验输入指纹一致）；</li>
                <li>选择相同的场景、方法与参数，在种子栏填入上方记录的随机种子；</li>
                <li>重新计算，得到的分配与本次逐动物一致。</li>
              </ol>
              <p className="text-xs text-muted-foreground">
                以上信息已随导出文件写入《汇总信息》表，归档请以导出文件为准。
              </p>
            </div>
          )}
```

- [ ] **Step 4: Build and manually verify**

Run: `bun run build`, then `bun run tauri dev`:
1. Exploratory + `ConstrainedRandom`, seed 42 → compute → 再抽一签 twice → three draw buttons, switching updates every card, export uses the selected draw (check 抽签序号 in the exported 汇总信息).
2. Redraw determinism: restart the flow with the same seed, redraw once — 第 2 签 must be identical to the previous session's 第 2 签.
3. GLP scenario + `BlockedRandom` → switcher buttons and 再抽一签 disabled, the 分配隐藏 alert visible.
4. Optimized run → switcher shows 排名 #1…#N and switching works.
Expected: all four hold.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(frontend): candidate switching, redraws and the reproduction card"
```

---

### Task 10: Full gates and design-doc status

**Files:**
- Modify: `docs/randomization_interaction_design.md` (header status line)

- [ ] **Step 1: Run every gate**

From `src-tauri/`:

```bash
cargo fmt
cargo test --release
cargo test --release -- --ignored
cargo clippy --all-targets
```

From the repo root: `bun run build`.
Expected: everything passes; `--ignored` covers the real-data and `perf_` harnesses because this branch touched `core/grouping/`. Read the output — do not claim success without it.

- [ ] **Step 2: Update the design doc status**

In `docs/randomization_interaction_design.md`, change `> 状态: 方案已确认，待实施` to `> 状态: 已实施`.

- [ ] **Step 3: Commit**

```bash
git add docs/randomization_interaction_design.md
git commit -m "docs: mark the randomization interaction design as implemented"
```
