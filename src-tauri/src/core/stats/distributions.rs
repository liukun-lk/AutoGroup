//! Distributions the post-hoc tests need and `statrs` does not provide.
//!
//! Tukey HSD and Dunnett's T3 both average a normal-theory probability over the sampling
//! distribution of the pooled standard deviation `s = sqrt(chi2_nu / nu)`:
//!
//! * Tukey uses the **studentized range** `q(k, nu)`.
//! * Dunnett's T3 uses the **studentized maximum modulus** `smm(c, nu)` over `c` comparisons.
//!
//! Both are integrals with no closed form, so they are evaluated by composite
//! Gauss-Legendre quadrature. This module is a direct port of the reference implementation in
//! `.claude/skills/animal-grouping/scripts/grouping_engine.py` (`srange_sf` / `smm_sf` /
//! `_chi_scale_integral`), which is validated against published critical value tables and
//! Monte-Carlo simulation by its `self-test` command. Keeping the two ports structurally
//! identical is what makes the Python side usable as an oracle for this one.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use statrs::function::erf::erfc;
use statrs::function::gamma::ln_gamma;

const GAUSS_NODES: usize = 12;

fn norm_cdf(z: f64) -> f64 {
    0.5 * erfc(-z / std::f64::consts::SQRT_2)
}

fn norm_pdf(z: f64) -> f64 {
    const INV_SQRT_2PI: f64 = 0.398_942_280_401_432_7;
    INV_SQRT_2PI * (-0.5 * z * z).exp()
}

/// Legendre polynomial `P_n(x)` and its derivative, by the standard recurrence.
fn legendre(n: usize, x: f64) -> (f64, f64) {
    let (mut p_prev, mut p_curr) = (1.0, x);
    for j in 2..=n {
        let j = j as f64;
        let next = ((2.0 * j - 1.0) * x * p_curr - (j - 1.0) * p_prev) / j;
        p_prev = p_curr;
        p_curr = next;
    }
    let dp = n as f64 * (x * p_curr - p_prev) / (x * x - 1.0);
    (p_curr, dp)
}

/// Gauss-Legendre nodes and weights on `[-1, 1]`, computed once by Newton iteration.
fn gauss_legendre() -> &'static ([f64; GAUSS_NODES], [f64; GAUSS_NODES]) {
    static TABLE: OnceLock<([f64; GAUSS_NODES], [f64; GAUSS_NODES])> = OnceLock::new();

    TABLE.get_or_init(|| {
        let mut nodes = [0.0; GAUSS_NODES];
        let mut weights = [0.0; GAUSS_NODES];

        for i in 1..=GAUSS_NODES {
            // Chebyshev approximation of the i-th root, refined by Newton's method.
            let mut x =
                (std::f64::consts::PI * (i as f64 - 0.25) / (GAUSS_NODES as f64 + 0.5)).cos();
            for _ in 0..100 {
                let (p, dp) = legendre(GAUSS_NODES, x);
                let dx = -p / dp;
                x += dx;
                if dx.abs() < 1e-15 {
                    break;
                }
            }
            let (_, dp) = legendre(GAUSS_NODES, x);
            nodes[i - 1] = x;
            weights[i - 1] = 2.0 / ((1.0 - x * x) * dp * dp);
        }

        (nodes, weights)
    })
}

/// Composite Gauss-Legendre integration of a smooth integrand over `[a, b]`.
fn integrate<F: Fn(f64) -> f64>(func: F, a: f64, b: f64, panels: usize) -> f64 {
    if b <= a {
        return 0.0;
    }

    let (nodes, weights) = gauss_legendre();
    let h = (b - a) / panels as f64;
    let half = h / 2.0;
    let mut total = 0.0;

    for panel in 0..panels {
        let mid = a + h * panel as f64 + half;
        let mut acc = 0.0;
        for (x, w) in nodes.iter().zip(weights.iter()) {
            acc += w * func(mid + half * x);
        }
        total += acc;
    }

    total * half
}

/// Integrate `func(s)` against the density of `s = sqrt(chi2_nu / nu)`.
///
/// This is the outer integral shared by both post-hoc distributions: it averages a
/// normal-theory probability over the uncertainty in the estimated standard deviation.
fn chi_scale_integral<F: Fn(f64) -> f64>(func: F, nu: f64) -> f64 {
    let log_c = 0.5 * nu * nu.ln() - ln_gamma(nu / 2.0) - (nu / 2.0 - 1.0) * std::f64::consts::LN_2;

    let integrand = |s: f64| {
        if s <= 0.0 {
            return 0.0;
        }
        let log_dens = log_c + (nu - 1.0) * s.ln() - nu * s * s / 2.0;
        if log_dens < -700.0 {
            return 0.0;
        }
        log_dens.exp() * func(s)
    };

    // The density of s concentrates around 1 with width ~1/sqrt(nu); 12 sigma each way
    // captures it to well past double precision.
    let spread = 12.0 / nu.sqrt();
    integrate(integrand, (1.0 - spread).max(0.0), 1.0 + spread, 16)
}

// --- Studentized range (Tukey HSD) -----------------------------------------

/// `P(range of k iid N(0,1) <= w)`.
fn range_prob(w: f64, k: usize) -> f64 {
    if w <= 0.0 {
        return 0.0;
    }

    let exponent = k as i32 - 1;
    let integrand = |z: f64| {
        let d = norm_cdf(z) - norm_cdf(z - w);
        if d <= 0.0 {
            return 0.0;
        }
        norm_pdf(z) * d.powi(exponent)
    };

    let (lo, hi) = (-8.5, 8.5 + w);
    let panels = ((hi - lo) as usize).max(20);
    k as f64 * integrate(integrand, lo, hi, panels)
}

const RANGE_STEP: f64 = 0.05;
const RANGE_MAX_W: f64 = 32.0;

/// Tabulated [`range_prob`] for one `k`, with 4-point Lagrange interpolation.
///
/// The studentized range p-value is a double integral whose inner integral depends only on
/// `(w, k)`. Tabulating that inner integral once per `k` turns every subsequent p-value into
/// a handful of table lookups, which is what keeps exact Tukey p-values affordable.
struct RangeProbTable {
    values: Vec<f64>,
}

impl RangeProbTable {
    fn new(k: usize) -> Self {
        let n = (RANGE_MAX_W / RANGE_STEP) as usize + 1;
        Self {
            values: (0..n)
                .map(|i| range_prob(i as f64 * RANGE_STEP, k))
                .collect(),
        }
    }

    fn eval(&self, w: f64) -> f64 {
        if w <= 0.0 {
            return 0.0;
        }
        if w >= RANGE_MAX_W {
            return 1.0;
        }

        let pos = w / RANGE_STEP;
        let i = (pos as usize).saturating_sub(1).min(self.values.len() - 4);
        let t = pos - i as f64;
        let (y0, y1, y2, y3) = (
            self.values[i],
            self.values[i + 1],
            self.values[i + 2],
            self.values[i + 3],
        );

        // Lagrange interpolation on the uniform nodes 0, 1, 2, 3.
        let p = -y0 * (t - 1.0) * (t - 2.0) * (t - 3.0) / 6.0
            + y1 * t * (t - 2.0) * (t - 3.0) / 2.0
            - y2 * t * (t - 1.0) * (t - 3.0) / 2.0
            + y3 * t * (t - 1.0) * (t - 2.0) / 6.0;

        p.clamp(0.0, 1.0)
    }
}

fn range_table(k: usize) -> Arc<RangeProbTable> {
    static CACHE: OnceLock<RwLock<HashMap<usize, Arc<RangeProbTable>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    if let Some(table) = cache.read().expect("range table cache poisoned").get(&k) {
        return Arc::clone(table);
    }

    // A concurrent miss may build the table twice; the result is identical either way, so
    // this stays cheaper than holding the write lock across the (millisecond-scale) build.
    let table = Arc::new(RangeProbTable::new(k));
    Arc::clone(
        cache
            .write()
            .expect("range table cache poisoned")
            .entry(k)
            .or_insert(table),
    )
}

/// Upper tail of the studentized range distribution: `P(q(k, nu) > q)`.
pub fn srange_sf(q: f64, k: usize, nu: f64) -> f64 {
    if !q.is_finite() || !nu.is_finite() || nu <= 0.0 || k < 2 {
        return f64::NAN;
    }
    if q <= 0.0 {
        return 1.0;
    }

    let table = range_table(k);
    let cdf = chi_scale_integral(|s| table.eval(q * s), nu);
    (1.0 - cdf).clamp(0.0, 1.0)
}

/// The `q` at which [`srange_sf`] equals `alpha`, by bisection on its monotone tail.
///
/// Cached, because the scoring pass needs this same threshold once per candidate while `k`
/// and `nu` stay fixed for a whole run.
pub fn srange_crit(alpha: f64, k: usize, nu: f64) -> f64 {
    /// `(alpha bits, k, nu bits)` — floats keyed by bit pattern so the cache stays exact.
    type CritKey = (u64, usize, u64);

    static CACHE: OnceLock<RwLock<HashMap<CritKey, f64>>> = OnceLock::new();

    // A run holds alpha and k fixed and nearly always the same within-group df, so the
    // scoring pass asks for one key over and over. A single-entry per-thread memo keeps the
    // shared lock — and its cross-core contention under rayon — out of the hot loop.
    thread_local! {
        static LAST: std::cell::Cell<Option<(CritKey, f64)>> =
            const { std::cell::Cell::new(None) };
    }

    let key = (alpha.to_bits(), k, nu.to_bits());

    if let Some(crit) = LAST.with(|last| {
        last.get()
            .and_then(|(cached_key, crit)| (cached_key == key).then_some(crit))
    }) {
        return crit;
    }

    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    if let Some(&crit) = cache.read().expect("crit cache poisoned").get(&key) {
        LAST.with(|last| last.set(Some((key, crit))));
        return crit;
    }

    let (mut lo, mut hi) = (0.0_f64, 20.0_f64);
    while srange_sf(hi, k, nu) > alpha && hi < 400.0 {
        hi *= 2.0;
    }
    for _ in 0..80 {
        let mid = (lo + hi) / 2.0;
        if srange_sf(mid, k, nu) > alpha {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-9 {
            break;
        }
    }
    let crit = (lo + hi) / 2.0;

    cache
        .write()
        .expect("crit cache poisoned")
        .insert(key, crit);
    LAST.with(|last| last.set(Some((key, crit))));
    crit
}

// --- Studentized maximum modulus (Dunnett's T3) ----------------------------

/// Upper tail of the studentized maximum modulus over `c` comparisons.
pub fn smm_sf(m: f64, c: usize, nu: f64) -> f64 {
    if !m.is_finite() || !nu.is_finite() || nu <= 0.0 || c == 0 {
        return f64::NAN;
    }
    if m <= 0.0 {
        return 1.0;
    }

    let exponent = c as i32;
    let cdf = chi_scale_integral(
        |s| {
            let base = 2.0 * norm_cdf(m * s) - 1.0;
            if base <= 0.0 {
                0.0
            } else {
                base.powi(exponent)
            }
        },
        nu,
    );

    (1.0 - cdf).clamp(0.0, 1.0)
}

/// Decide `smm_sf(m, c, nu) > alpha` while skipping the integral whenever cheap bounds settle it.
///
/// `smm_sf` is bracketed by one uncorrected two-tailed t probability (lower bound) and its
/// Bonferroni multiple (upper bound), so a single t-distribution lookup decides most
/// comparisons. Only the narrow band between the bounds pays for the quadrature — which
/// matters because Dunnett's T3 uses a Welch degrees of freedom that differs per comparison
/// and therefore cannot be cached the way [`srange_crit`] is.
///
/// The verdict is identical to comparing the exact tail against `alpha`, so the scoring pass
/// and the reported p-values cannot disagree about which candidates are valid.
pub fn smm_exceeds(m: f64, c: usize, nu: f64, alpha: f64) -> bool {
    use statrs::distribution::{ContinuousCDF, StudentsT};

    if !m.is_finite() || !nu.is_finite() || nu <= 0.0 || c == 0 {
        // Degenerate input: the exact tail is NaN, which never exceeds alpha.
        return false;
    }

    let Ok(dist) = StudentsT::new(0.0, 1.0, nu) else {
        return false;
    };

    let single = 2.0 * (1.0 - dist.cdf(m.abs()));
    if single > alpha {
        return true;
    }
    if single * c as f64 <= alpha {
        return false;
    }

    smm_sf(m, c, nu) > alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Published Tukey critical values (Harter's tables), to 3 decimals.
    #[test]
    fn srange_crit_matches_published_tables() {
        let cases = [
            (3usize, 12.0, 3.773),
            (4, 20.0, 3.958),
            (5, 10.0, 4.654),
            (3, 6.0, 4.339),
        ];

        for (k, nu, expected) in cases {
            let actual = srange_crit(0.05, k, nu);
            assert!(
                (actual - expected).abs() < 5e-3,
                "q_0.05({k}, {nu}): expected {expected}, got {actual}"
            );
        }
    }

    /// For k = 2 the studentized range reduces to the two-tailed t distribution at q/sqrt(2).
    #[test]
    fn srange_reduces_to_two_tailed_t_at_k2() {
        use statrs::distribution::{ContinuousCDF, StudentsT};

        for (q, nu) in [(2.0, 5.0), (3.0, 10.0), (4.0, 20.0), (1.5, 8.0)] {
            let dist = StudentsT::new(0.0, 1.0, nu).unwrap();
            let expected = 2.0 * (1.0 - dist.cdf(q / std::f64::consts::SQRT_2));
            let actual = srange_sf(q, 2, nu);
            assert!(
                (actual - expected).abs() < 1e-6,
                "srange_sf({q}, 2, {nu}): expected {expected}, got {actual}"
            );
        }
    }

    /// For a single comparison the maximum modulus reduces to the two-tailed t distribution.
    #[test]
    fn smm_reduces_to_two_tailed_t_at_c1() {
        use statrs::distribution::{ContinuousCDF, StudentsT};

        for (m, nu) in [(2.0, 4.0), (2.5, 8.0), (3.0, 10.0), (1.0, 15.0)] {
            let dist = StudentsT::new(0.0, 1.0, nu).unwrap();
            let expected = 2.0 * (1.0 - dist.cdf(m));
            let actual = smm_sf(m, 1, nu);
            assert!(
                (actual - expected).abs() < 1e-6,
                "smm_sf({m}, 1, {nu}): expected {expected}, got {actual}"
            );
        }
    }

    /// The maximum modulus must sit between one uncorrected comparison and its
    /// Bonferroni bound. `dunnett_t3_into` relies on exactly this to skip the integral.
    #[test]
    fn smm_is_bracketed_by_single_and_bonferroni() {
        use statrs::distribution::{ContinuousCDF, StudentsT};

        for (m, c, nu) in [(2.5, 3usize, 4.0), (3.0, 3, 8.0), (2.5, 6, 10.0)] {
            let dist = StudentsT::new(0.0, 1.0, nu).unwrap();
            let single = 2.0 * (1.0 - dist.cdf(m));
            let smm = smm_sf(m, c, nu);
            assert!(
                smm >= single - 1e-9 && smm <= single * c as f64 + 1e-9,
                "smm_sf({m}, {c}, {nu}) = {smm} outside [{single}, {}]",
                single * c as f64
            );
        }
    }

    /// Values quantified in `references/statistics.md` as the exact answers the old
    /// approximations were missing.
    #[test]
    fn matches_reference_implementation_values() {
        for (q, k, nu, expected) in [
            (3.0, 3usize, 6.0, 0.16546),
            (4.0, 3, 12.0, 0.03764),
            (5.0, 4, 8.0, 0.03139),
            (4.0, 5, 15.0, 0.08046),
        ] {
            let actual = srange_sf(q, k, nu);
            assert!(
                (actual - expected).abs() < 1e-4,
                "srange_sf({q}, {k}, {nu}): expected {expected}, got {actual}"
            );
        }

        for (t, c, nu, expected) in [
            (2.5, 3usize, 4.0, 0.15961),
            (3.0, 3, 8.0, 0.04698),
            (2.5, 6, 10.0, 0.15381),
        ] {
            let actual = smm_sf(t, c, nu);
            assert!(
                (actual - expected).abs() < 1e-4,
                "smm_sf({t}, {c}, {nu}): expected {expected}, got {actual}"
            );
        }
    }

    /// `srange_crit` must be the exact inverse of `srange_sf`, since the scoring pass uses
    /// the threshold while the reported p-value uses the tail. If they disagreed, a
    /// candidate could rank as valid and then report a failing comparison.
    #[test]
    fn srange_crit_inverts_srange_sf() {
        for (k, nu) in [(3usize, 6.0), (4, 12.0), (5, 20.0)] {
            let crit = srange_crit(0.05, k, nu);
            assert!(srange_sf(crit - 1e-6, k, nu) > 0.05);
            assert!(srange_sf(crit + 1e-6, k, nu) <= 0.05);
        }
    }

    /// The bounds-based shortcut must agree with the exact tail everywhere, including inside
    /// the band where it falls through to the integral.
    #[test]
    fn smm_exceeds_agrees_with_exact_tail() {
        for c in [3usize, 6] {
            for nu in [4.0, 8.0, 12.5, 30.0] {
                let mut m = 0.1;
                while m < 6.0 {
                    let expected = smm_sf(m, c, nu) > 0.05;
                    assert_eq!(
                        smm_exceeds(m, c, nu, 0.05),
                        expected,
                        "smm_exceeds({m}, {c}, {nu}) disagreed with the exact tail"
                    );
                    m += 0.05;
                }
            }
        }
    }

    #[test]
    fn degenerate_inputs_yield_nan() {
        assert!(srange_sf(2.0, 3, f64::NAN).is_nan());
        assert!(srange_sf(2.0, 3, 0.0).is_nan());
        assert!(smm_sf(2.0, 3, f64::NAN).is_nan());
        assert!(smm_sf(f64::NAN, 3, 5.0).is_nan());
    }
}
