use anyhow::Result;

use super::distributions::{smm_exceeds, smm_sf};

/// Dunnett's T3 test
/// Post-hoc test for Welch ANOVA when variances are unequal
/// Returns pairwise comparisons: Vec of (group1_idx, group2_idx, p_value)
pub fn dunnett_t3(groups: &[Vec<f64>]) -> Result<Vec<(usize, usize, f64)>> {
    let mut results = Vec::new();
    dunnett_t3_into(groups, &mut results)?;
    Ok(results)
}

/// Same as [`dunnett_t3`] but appends into a caller-owned buffer, so hot loops can
/// reuse one allocation across indicators and candidates.
///
/// Each pair uses a Welch t statistic with Welch-Satterthwaite degrees of freedom, then the
/// **studentized maximum modulus** over all `C = k(k-1)/2` comparisons for multiplicity.
/// Without that last step this would be a bare pairwise Welch t, not T3.
pub fn dunnett_t3_into(groups: &[Vec<f64>], results: &mut Vec<(usize, usize, f64)>) -> Result<()> {
    let k = groups.len();
    if k < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Dunnett's T3"));
    }

    let group_stats = group_statistics(groups);
    let num_comparisons = k * (k - 1) / 2;

    for i in 0..k {
        for j in (i + 1)..k {
            let (t_stat, df) = welch_pair(group_stats[i], group_stats[j]);
            results.push((i, j, smm_sf(t_stat, num_comparisons, df)));
        }
    }

    Ok(())
}

/// Whether every pairwise comparison clears `alpha`, without always computing exact p-values.
///
/// See [`smm_exceeds`] for why this is both exact and much cheaper: the Welch df differs per
/// comparison, so unlike Tukey there is no reusable critical value to cache.
pub fn dunnett_t3_all_valid(groups: &[Vec<f64>], alpha: f64) -> Result<bool> {
    let k = groups.len();
    if k < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Dunnett's T3"));
    }

    let group_stats = group_statistics(groups);
    let num_comparisons = k * (k - 1) / 2;

    for i in 0..k {
        for j in (i + 1)..k {
            let (t_stat, df) = welch_pair(group_stats[i], group_stats[j]);
            if !smm_exceeds(t_stat, num_comparisons, df, alpha) {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// Per-group `(mean, unbiased variance, n)`.
fn group_statistics(groups: &[Vec<f64>]) -> Vec<(f64, f64, usize)> {
    groups
        .iter()
        .map(|g| {
            let n = g.len() as f64;
            let mean = g.iter().sum::<f64>() / n;
            let variance = g.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
            (mean, variance, g.len())
        })
        .collect()
}

/// Welch t statistic (absolute) and Welch-Satterthwaite degrees of freedom for one pair.
fn welch_pair(a: (f64, f64, usize), b: (f64, f64, usize)) -> (f64, f64) {
    let (mean_a, var_a, n_a) = a;
    let (mean_b, var_b, n_b) = b;

    let se = (var_a / n_a as f64 + var_b / n_b as f64).sqrt();
    let t_stat = ((mean_a - mean_b) / se).abs();

    (t_stat, welch_df(var_a, n_a, var_b, n_b))
}

/// Calculate Welch-Satterthwaite degrees of freedom
fn welch_df(var1: f64, n1: usize, var2: f64, n2: usize) -> f64 {
    let n1 = n1 as f64;
    let n2 = n2 as f64;

    let numerator = (var1 / n1 + var2 / n2).powi(2);
    let denominator = (var1 / n1).powi(2) / (n1 - 1.0) + (var2 / n2).powi(2) / (n2 - 1.0);

    numerator / denominator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dunnett_t3_similar_groups() {
        // Groups with similar means but different variances
        let groups = vec![
            vec![1.0, 1.1, 1.2], // Low variance
            vec![2.0, 3.0, 4.0], // Higher variance
            vec![1.5, 1.6, 1.7], // Low variance
        ];

        let results = dunnett_t3(&groups).unwrap();

        // Should have C(3,2) = 3 comparisons
        assert_eq!(results.len(), 3);

        for (i, j, p) in &results {
            println!("Group {i} vs {j}: P = {p}");
        }
    }

    #[test]
    fn test_dunnett_t3_different_groups() {
        // Groups with different means
        let groups = vec![
            vec![1.0, 1.1, 1.2, 1.3],
            vec![5.0, 5.1, 5.2, 5.3],
            vec![10.0, 10.1, 10.2, 10.3],
        ];

        let results = dunnett_t3(&groups).unwrap();

        // All comparisons should show significant differences
        for (i, j, p) in &results {
            println!("Group {i} vs {j}: P = {p}");
            assert!(*p < 0.01, "Should detect significant difference");
        }
    }

    /// Regression guard: T3 must be more conservative than an uncorrected pairwise Welch t.
    /// The previous implementation returned the uncorrected tail, so its p-values were only
    /// ~0.2-0.4x the true ones and flagged balanced groupings as imbalanced.
    #[test]
    fn dunnett_t3_is_more_conservative_than_bare_welch_t() {
        use statrs::distribution::{ContinuousCDF, StudentsT};

        let groups = vec![
            vec![1.0, 1.4, 1.1, 1.3],
            vec![2.0, 3.0, 2.4, 2.9],
            vec![1.5, 1.7, 2.2, 1.6],
        ];

        let stats = group_statistics(&groups);
        let results = dunnett_t3(&groups).unwrap();

        for &(i, j, p) in &results {
            let (t_stat, df) = welch_pair(stats[i], stats[j]);
            let dist = StudentsT::new(0.0, 1.0, df).unwrap();
            let uncorrected = 2.0 * (1.0 - dist.cdf(t_stat));
            assert!(
                p >= uncorrected - 1e-12,
                "group {i} vs {j}: corrected P = {p} is below the uncorrected {uncorrected}"
            );
        }
    }

    /// The fast verdict must match the exact p-values, including near the alpha boundary.
    #[test]
    fn dunnett_all_valid_agrees_with_exact_p_values() {
        let cases = [
            vec![
                vec![1.0, 1.1, 1.2],
                vec![2.0, 3.0, 4.0],
                vec![1.5, 1.6, 1.7],
            ],
            vec![
                vec![1.0, 1.1, 1.2, 1.3],
                vec![5.0, 5.1, 5.2, 5.3],
                vec![10.0, 10.1, 10.2, 10.3],
            ],
            vec![
                vec![1.0, 1.4, 1.1, 1.3],
                vec![2.0, 3.0, 2.4, 2.9],
                vec![1.5, 1.7, 2.2, 1.6],
            ],
            vec![
                vec![10.0, 12.0, 11.0, 13.0],
                vec![14.0, 16.5, 15.0, 17.0],
                vec![11.0, 13.0, 12.0, 14.0],
                vec![12.0, 14.0, 13.0, 15.5],
            ],
        ];

        for groups in &cases {
            for alpha in [0.01, 0.05, 0.1] {
                let exact = dunnett_t3(groups).unwrap();
                let expected = exact.iter().all(|&(_, _, p)| p > alpha);
                assert_eq!(
                    dunnett_t3_all_valid(groups, alpha).unwrap(),
                    expected,
                    "verdict disagreed with exact p-values {exact:?} at alpha = {alpha}"
                );
            }
        }
    }

    #[test]
    fn test_welch_df_calculation() {
        // Test degrees of freedom calculation
        let df = welch_df(1.0, 10, 4.0, 15);
        assert!(df > 0.0, "DF should be positive");
        assert!(df < 24.0, "DF should be less than n1 + n2 - 2");
    }
}
