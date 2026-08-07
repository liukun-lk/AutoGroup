use anyhow::Result;
use statrs::distribution::{ContinuousCDF, StudentsT};

/// P-value for a comparison whose standard error is zero: both groups are constant, so
/// the t statistic degenerates to 0/0 or +/-inf and `StudentsT::cdf` panics on the former.
/// With no spread left there is nothing to test — either the two constants coincide or
/// they differ, with no uncertainty in between.
fn degenerate_p(mean1: f64, mean2: f64) -> f64 {
    if mean1 == mean2 {
        1.0
    } else {
        0.0
    }
}

/// Student's t-test (assumes equal variances)
/// Tests if means of two groups are significantly different
/// Returns P-value (two-tailed)
pub fn student_ttest(group1: &[f64], group2: &[f64]) -> Result<f64> {
    if group1.len() < 2 || group2.len() < 2 {
        return Err(anyhow::anyhow!(
            "Each group must have at least 2 observations"
        ));
    }

    let n1 = group1.len() as f64;
    let n2 = group2.len() as f64;

    // Calculate means
    let mean1 = group1.iter().sum::<f64>() / n1;
    let mean2 = group2.iter().sum::<f64>() / n2;

    // Calculate variances
    let var1 = group1.iter().map(|x| (x - mean1).powi(2)).sum::<f64>() / (n1 - 1.0);
    let var2 = group2.iter().map(|x| (x - mean2).powi(2)).sum::<f64>() / (n2 - 1.0);

    // Pooled standard deviation
    let pooled_variance = ((n1 - 1.0) * var1 + (n2 - 1.0) * var2) / (n1 + n2 - 2.0);
    let se = (pooled_variance * (1.0 / n1 + 1.0 / n2)).sqrt();

    if se <= 0.0 {
        return Ok(degenerate_p(mean1, mean2));
    }

    // t-statistic
    let t_stat = (mean1 - mean2) / se;

    // Degrees of freedom
    let df = n1 + n2 - 2.0;

    // P-value (two-tailed)
    let t_dist = StudentsT::new(0.0, 1.0, df)?;
    let p_value = 2.0 * (1.0 - t_dist.cdf(t_stat.abs()));

    Ok(p_value)
}

/// Welch's t-test (does not assume equal variances)
/// More robust than Student's t-test when variances differ
/// Returns P-value (two-tailed)
pub fn welch_ttest(group1: &[f64], group2: &[f64]) -> Result<f64> {
    if group1.len() < 2 || group2.len() < 2 {
        return Err(anyhow::anyhow!(
            "Each group must have at least 2 observations"
        ));
    }

    let n1 = group1.len() as f64;
    let n2 = group2.len() as f64;

    // Calculate means
    let mean1 = group1.iter().sum::<f64>() / n1;
    let mean2 = group2.iter().sum::<f64>() / n2;

    // Calculate variances
    let var1 = group1.iter().map(|x| (x - mean1).powi(2)).sum::<f64>() / (n1 - 1.0);
    let var2 = group2.iter().map(|x| (x - mean2).powi(2)).sum::<f64>() / (n2 - 1.0);

    // Standard error
    let se = (var1 / n1 + var2 / n2).sqrt();

    if se <= 0.0 {
        return Ok(degenerate_p(mean1, mean2));
    }

    // t-statistic
    let t_stat = (mean1 - mean2) / se;

    // Welch-Satterthwaite degrees of freedom
    let df = (var1 / n1 + var2 / n2).powi(2)
        / ((var1 / n1).powi(2) / (n1 - 1.0) + (var2 / n2).powi(2) / (n2 - 1.0));

    // P-value (two-tailed)
    let t_dist = StudentsT::new(0.0, 1.0, df)?;
    let p_value = 2.0 * (1.0 - t_dist.cdf(t_stat.abs()));

    Ok(p_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both groups constant: the standard error vanishes and the t statistic is 0/0 or
    /// +/-inf. The answer is decided by the means, not by a distribution lookup.
    #[test]
    fn zero_standard_error_is_decided_by_the_means() {
        let a = vec![1.0; 4];
        let b = vec![1.0; 4];
        assert_eq!(student_ttest(&a, &b).unwrap(), 1.0);
        assert_eq!(welch_ttest(&a, &b).unwrap(), 1.0);

        let c = vec![2.0; 4];
        assert_eq!(student_ttest(&a, &c).unwrap(), 0.0);
        assert_eq!(welch_ttest(&a, &c).unwrap(), 0.0);
    }

    #[test]
    fn test_student_ttest_same_distribution() {
        // Two samples from same distribution should have high P-value
        let group1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let group2 = vec![1.5, 2.5, 3.5, 4.5, 5.5];

        let p = student_ttest(&group1, &group2).unwrap();
        assert!(
            p > 0.05,
            "P-value should be > 0.05 for similar distributions"
        );
    }

    #[test]
    fn test_student_ttest_different_distributions() {
        // Two samples with very different means
        let group1 = vec![1.0, 1.1, 1.2, 1.3, 1.4];
        let group2 = vec![10.0, 10.1, 10.2, 10.3, 10.4];

        let p = student_ttest(&group1, &group2).unwrap();
        assert!(
            p < 0.001,
            "P-value should be < 0.001 for very different means"
        );
    }

    #[test]
    fn test_welch_ttest_same_distribution() {
        let group1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let group2 = vec![1.5, 2.5, 3.5, 4.5, 5.5];

        let p = welch_ttest(&group1, &group2).unwrap();
        assert!(
            p > 0.05,
            "P-value should be > 0.05 for similar distributions"
        );
    }

    #[test]
    fn test_welch_ttest_different_distributions() {
        let group1 = vec![1.0, 1.1, 1.2, 1.3, 1.4];
        let group2 = vec![10.0, 10.1, 10.2, 10.3, 10.4];

        let p = welch_ttest(&group1, &group2).unwrap();
        assert!(
            p < 0.001,
            "P-value should be < 0.001 for very different means"
        );
    }

    #[test]
    fn test_welch_vs_student_equal_variances() {
        // When variances are equal, both tests should give similar results
        let group1 = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let group2 = vec![2.0, 3.0, 4.0, 5.0, 6.0];

        let p_student = student_ttest(&group1, &group2).unwrap();
        let p_welch = welch_ttest(&group1, &group2).unwrap();

        println!("Student's t: {p_student}, Welch's t: {p_welch}");
        assert!(
            (p_student - p_welch).abs() < 0.1,
            "Results should be similar"
        );
    }
}
