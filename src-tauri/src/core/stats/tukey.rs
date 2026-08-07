use anyhow::Result;

use super::distributions::{srange_crit, srange_sf};

/// Tukey's Honest Significant Difference (HSD) test
/// Post-hoc test for ANOVA when variances are equal
/// Returns pairwise comparisons: Vec of (group1_idx, group2_idx, p_value)
pub fn tukey_hsd(groups: &[Vec<f64>]) -> Result<Vec<(usize, usize, f64)>> {
    let mut results = Vec::new();
    tukey_hsd_into(groups, &mut results)?;
    Ok(results)
}

/// Same as [`tukey_hsd`] but appends into a caller-owned buffer, so hot loops can
/// reuse one allocation across indicators and candidates.
///
/// P-values come from the studentized range distribution, which is the only correct source
/// for Tukey HSD. Each one costs a quadrature, so prefer [`tukey_all_valid`] when only the
/// pass/fail verdict is needed.
pub fn tukey_hsd_into(groups: &[Vec<f64>], results: &mut Vec<(usize, usize, f64)>) -> Result<()> {
    let k = groups.len();
    if k < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Tukey HSD"));
    }

    let (group_stats, mse, df_within) = pooled_within(groups);

    for i in 0..k {
        for j in (i + 1)..k {
            let p = match pairwise(mse, group_stats[i], group_stats[j]) {
                Pairwise::Q(q_stat) => srange_sf(q_stat, k, df_within),
                Pairwise::Degenerate(p) => p,
            };
            results.push((i, j, p));
        }
    }

    Ok(())
}

/// Whether every pairwise comparison clears `alpha`, without computing exact p-values.
///
/// The studentized range tail is monotone decreasing in `q`, so comparing against the cached
/// critical value gives exactly the same verdict as `srange_sf(q, k, df) > alpha` — at the
/// cost of one bisection per `(alpha, k, df)` for a whole run instead of one quadrature per
/// comparison. This is what keeps the scoring pass affordable over 10^5+ candidates.
pub fn tukey_all_valid(groups: &[Vec<f64>], alpha: f64) -> Result<bool> {
    let k = groups.len();
    if k < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Tukey HSD"));
    }

    let (group_stats, mse, df_within) = pooled_within(groups);
    let crit = srange_crit(alpha, k, df_within);

    for i in 0..k {
        for j in (i + 1)..k {
            let passes = match pairwise(mse, group_stats[i], group_stats[j]) {
                Pairwise::Q(q_stat) => q_stat < crit,
                Pairwise::Degenerate(p) => p > alpha,
            };
            if !passes {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// Group `(mean, n)` pairs, the pooled within-group mean square, and its degrees of freedom.
///
/// Shared by the exact and the validity-only paths so the two cannot drift apart.
fn pooled_within(groups: &[Vec<f64>]) -> (Vec<(f64, usize)>, f64, f64) {
    let group_stats: Vec<(f64, usize)> = groups
        .iter()
        .map(|g| {
            let n = g.len();
            (g.iter().sum::<f64>() / n as f64, n)
        })
        .collect();

    let mut ss_within = 0.0;
    let mut df_within = 0usize;

    for (group, &(mean, _)) in groups.iter().zip(&group_stats) {
        ss_within += group.iter().map(|x| (x - mean).powi(2)).sum::<f64>();
        df_within += group.len() - 1;
    }

    (group_stats, ss_within / df_within as f64, df_within as f64)
}

/// One pairwise comparison, either as a q statistic or already decided.
enum Pairwise {
    Q(f64),
    /// Zero pooled within-group variance: q would be 0/0 or x/0, so the comparison is
    /// settled by whether the two group means coincide. Carries the p-value directly.
    Degenerate(f64),
}

/// Tukey's q statistic for one pair of groups.
///
/// Shared by the exact and the validity-only paths, degenerate case included — the two
/// must reach the same verdict, and the earlier code did not: a NaN q read as "failed"
/// on the shortcut while the exact path reported a NaN p-value for a pair of identical
/// groups, which is the most balanced outcome there is.
fn pairwise(mse: f64, (mean_i, n_i): (f64, usize), (mean_j, n_j): (f64, usize)) -> Pairwise {
    let se = (mse * (1.0 / n_i as f64 + 1.0 / n_j as f64) / 2.0).sqrt();

    if se <= 0.0 {
        return Pairwise::Degenerate(if mean_i == mean_j { 1.0 } else { 0.0 });
    }

    Pairwise::Q((mean_i - mean_j).abs() / se)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Zero pooled within-group variance makes the q statistic 0/0. The exact path used to
    /// report NaN for every pair, which silently reads as "failed"; both paths now decide
    /// on the means, and — as everywhere else in this module — they must agree.
    #[test]
    fn zero_pooled_variance_is_decided_by_the_means() {
        let identical = vec![vec![1.0; 4], vec![1.0; 4], vec![1.0; 4]];
        let p: Vec<f64> = tukey_hsd(&identical).unwrap().iter().map(|c| c.2).collect();
        assert_eq!(p, vec![1.0, 1.0, 1.0]);
        assert!(tukey_all_valid(&identical, 0.05).unwrap());

        let separated = vec![vec![1.0; 4], vec![2.0; 4], vec![3.0; 4]];
        let p: Vec<f64> = tukey_hsd(&separated).unwrap().iter().map(|c| c.2).collect();
        assert_eq!(p, vec![0.0, 0.0, 0.0]);
        assert!(!tukey_all_valid(&separated, 0.05).unwrap());

        // Two groups share one constant value, the third does not: the pairs that are
        // still comparable keep their real p-values.
        let mixed = vec![vec![1.0; 4], vec![1.0; 4], vec![3.0, 9.0, 4.0, 1.0]];
        let exact = tukey_hsd(&mixed).unwrap();
        assert_eq!(exact[0].2, 1.0);
        assert!(exact[1].2 > 0.0 && exact[1].2 < 1.0, "{exact:?}");
        assert_eq!(
            tukey_all_valid(&mixed, 0.05).unwrap(),
            exact.iter().all(|c| c.2 > 0.05),
            "the validity shortcut must agree with the exact p-values"
        );
    }

    #[test]
    fn test_tukey_hsd_similar_groups() {
        // Groups with similar means should have high P-values
        let groups = vec![
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            vec![1.5, 2.5, 3.5, 4.5, 5.5],
            vec![1.2, 2.2, 3.2, 4.2, 5.2],
        ];

        let results = tukey_hsd(&groups).unwrap();

        // Should have C(3,2) = 3 pairwise comparisons
        assert_eq!(results.len(), 3);

        // All P-values should be relatively high
        for (i, j, p) in &results {
            println!("Group {i} vs {j}: P = {p}");
            assert!(p > &0.05, "Expected high P-value for similar groups");
        }
    }

    /// The fast verdict must match the exact p-values everywhere, since the scoring pass uses
    /// the former to rank candidates and the export reports the latter.
    #[test]
    fn tukey_all_valid_agrees_with_exact_p_values() {
        let cases = [
            vec![
                vec![1.0, 2.0, 3.0, 4.0, 5.0],
                vec![1.5, 2.5, 3.5, 4.5, 5.5],
                vec![1.2, 2.2, 3.2, 4.2, 5.2],
            ],
            vec![
                vec![1.0, 1.1, 1.2, 1.3, 1.4],
                vec![1.5, 1.6, 1.7, 1.8, 1.9],
                vec![10.0, 10.1, 10.2, 10.3, 10.4],
            ],
            // Deliberately near the alpha = 0.05 boundary.
            vec![
                vec![1.0, 2.0, 3.0],
                vec![3.4, 4.4, 5.4],
                vec![2.2, 3.2, 4.2],
            ],
            vec![
                vec![10.0, 12.0, 11.0, 13.0],
                vec![14.0, 16.0, 15.0, 17.0],
                vec![11.0, 13.0, 12.0, 14.0],
                vec![12.0, 14.0, 13.0, 15.0],
            ],
        ];

        for groups in &cases {
            for alpha in [0.01, 0.05, 0.1] {
                let exact = tukey_hsd(groups).unwrap();
                let expected = exact.iter().all(|&(_, _, p)| p > alpha);
                assert_eq!(
                    tukey_all_valid(groups, alpha).unwrap(),
                    expected,
                    "verdict disagreed with exact p-values {exact:?} at alpha = {alpha}"
                );
            }
        }
    }

    /// Published critical value: q_0.05(3, 6) = 4.339. A q just under it must pass, just over
    /// it must fail. The old `q/sqrt(2)` approximation put this threshold at 4.649 and let
    /// genuinely imbalanced pairs through.
    #[test]
    fn tukey_p_values_track_the_published_critical_value() {
        // Three groups of three, unit MSE by construction: q = |mean_i - mean_j| / (1/sqrt(3)).
        let build = |shift: f64| {
            vec![
                vec![-1.0, 0.0, 1.0],
                vec![-1.0 + shift, shift, 1.0 + shift],
                vec![-1.0, 0.0, 1.0],
            ]
        };

        let mse = 1.0; // each group has variance 1
        let se = (mse * (1.0 / 3.0 + 1.0 / 3.0) / 2.0f64).sqrt();

        let below = build(4.30 * se);
        let above = build(4.40 * se);

        assert!(tukey_all_valid(&below, 0.05).unwrap(), "q = 4.30 < 4.339");
        assert!(!tukey_all_valid(&above, 0.05).unwrap(), "q = 4.40 > 4.339");
    }

    /// Regression guard for the old approximation, which multiplied a t-tail by k and clamped
    /// to 1.0, saturating every comparison on small samples.
    #[test]
    fn tukey_p_values_are_not_saturated_on_small_samples() {
        let groups = vec![
            vec![1.0, 2.0, 3.5],
            vec![1.4, 2.6, 3.1],
            vec![0.8, 2.1, 3.9],
        ];

        let results = tukey_hsd(&groups).unwrap();
        assert!(
            results.iter().all(|&(_, _, p)| p < 1.0),
            "post-hoc p-values collapsed to 1.0: {results:?}"
        );
    }

    #[test]
    fn test_tukey_hsd_different_groups() {
        // One group very different from others
        let groups = vec![
            vec![1.0, 1.1, 1.2, 1.3, 1.4],
            vec![1.5, 1.6, 1.7, 1.8, 1.9],
            vec![10.0, 10.1, 10.2, 10.3, 10.4],
        ];

        let results = tukey_hsd(&groups).unwrap();

        // Comparisons involving group 2 should have low P-values
        for (i, j, p) in &results {
            if *i == 2 || *j == 2 {
                println!("Group {i} vs {j}: P = {p}");
                // Should detect difference
                assert!(p < &0.05, "Should detect difference with outlier group");
            }
        }
    }
}
