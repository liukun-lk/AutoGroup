use anyhow::Result;
use statrs::distribution::{FisherSnedecor, ContinuousCDF};

/// One-way ANOVA (Analysis of Variance)
/// Tests if means of multiple groups are significantly different
/// Returns P-value
pub fn one_way_anova(groups: &[Vec<f64>]) -> Result<f64> {
    if groups.len() < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for ANOVA"));
    }

    // Calculate group statistics
    let k = groups.len() as f64; // number of groups
    let mut n_total = 0;
    let mut group_stats = Vec::new();

    for group in groups {
        let n = group.len();
        n_total += n;
        let sum: f64 = group.iter().sum();
        let mean = sum / n as f64;
        group_stats.push((n, sum, mean));
    }

    if n_total < 3 {
        return Err(anyhow::anyhow!("Need at least 3 total observations"));
    }

    // Grand mean
    let grand_sum: f64 = group_stats.iter().map(|(_, sum, _)| sum).sum();
    let grand_mean = grand_sum / n_total as f64;

    // Between-group sum of squares (SSB)
    let ssb: f64 = group_stats
        .iter()
        .map(|(n, _, mean)| (*n as f64) * (mean - grand_mean).powi(2))
        .sum();

    // Within-group sum of squares (SSW)
    let ssw: f64 = groups
        .iter()
        .zip(&group_stats)
        .map(|(group, (_, _, mean))| {
            group.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
        })
        .sum();

    // Degrees of freedom
    let df_between = k - 1.0;
    let df_within = n_total as f64 - k;

    // Mean squares
    let msb = ssb / df_between;
    let msw = ssw / df_within;

    // F-statistic
    let f_stat = msb / msw;

    // P-value from F-distribution
    let f_dist = FisherSnedecor::new(df_between, df_within)?;
    let p_value = 1.0 - f_dist.cdf(f_stat);

    Ok(p_value)
}

/// Welch's ANOVA (for unequal variances)
/// More robust when variances are heterogeneous
pub fn welch_anova(groups: &[Vec<f64>]) -> Result<f64> {
    if groups.len() < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Welch ANOVA"));
    }

    let k = groups.len() as f64;

    // Calculate group statistics
    let mut group_stats = Vec::new();
    for group in groups {
        let n = group.len() as f64;
        let mean = group.iter().sum::<f64>() / n;
        let variance = group
            .iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / (n - 1.0);
        let weight = n / variance;
        group_stats.push((n, mean, variance, weight));
    }

    // Weighted grand mean
    let sum_weights: f64 = group_stats.iter().map(|(_, _, _, w)| w).sum();
    let grand_mean: f64 = group_stats
        .iter()
        .map(|(_, mean, _, w)| w * mean)
        .sum::<f64>() / sum_weights;

    // Welch's F-statistic
    let numerator: f64 = group_stats
        .iter()
        .map(|(_, mean, _, w)| w * (mean - grand_mean).powi(2))
        .sum::<f64>() / (k - 1.0);

    let h: f64 = group_stats
        .iter()
        .map(|(n, _, _, w)| {
            let lambda = w / sum_weights;
            (1.0 - lambda).powi(2) / (n - 1.0)
        })
        .sum();

    let denominator = 1.0 + (2.0 * (k - 2.0) / (k * k - 1.0)) * h;

    let f_stat = numerator / denominator;

    // Degrees of freedom
    let df1 = k - 1.0;
    let df2 = 1.0 / (3.0 * h / (k * k - 1.0));

    // P-value
    let f_dist = FisherSnedecor::new(df1, df2)?;
    let p_value = 1.0 - f_dist.cdf(f_stat);

    Ok(p_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one_way_anova_equal_means() {
        // Groups with similar means should have high P-value
        let groups = vec![
            vec![1.0, 2.0, 3.0],
            vec![1.5, 2.5, 3.5],
            vec![1.2, 2.2, 3.2],
        ];

        let p = one_way_anova(&groups).unwrap();
        assert!(p > 0.05, "P-value should be > 0.05 for similar means");
    }

    #[test]
    fn test_one_way_anova_different_means() {
        // Groups with very different means should have low P-value
        let groups = vec![
            vec![1.0, 1.1, 1.2],
            vec![10.0, 10.1, 10.2],
            vec![20.0, 20.1, 20.2],
        ];

        let p = one_way_anova(&groups).unwrap();
        assert!(p < 0.001, "P-value should be < 0.001 for different means");
    }
}
