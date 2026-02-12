use crate::core::models::*;
use anyhow::{anyhow, Result};

/// Generate all possible groupings through exhaustive enumeration
/// Suitable for datasets with ≤50 animals
pub fn enumerate_all(
    animals: &[Animal],
    config: &GroupConfig,
) -> Result<Vec<CandidateGrouping>> {
    // Step 1: Validate configuration
    validate_config(animals, config)?;

    // Step 2: Separate animals by sex
    let male_indices: Vec<usize> = animals
        .iter()
        .enumerate()
        .filter(|(_, a)| a.sex == Sex::Male)
        .map(|(i, _)| i)
        .collect();

    let female_indices: Vec<usize> = animals
        .iter()
        .enumerate()
        .filter(|(_, a)| a.sex == Sex::Female)
        .map(|(i, _)| i)
        .collect();

    // Step 3: Generate all valid groupings
    let mut all_groupings = Vec::new();

    // For 2-group case (most common)
    if config.num_groups == 2 && config.sex_constraints.len() == 2 {
        let group1_constraint = &config.sex_constraints[0];
        let group2_constraint = &config.sex_constraints[1];

        // Generate combinations for group 1
        let male_combos_g1 = combinations(&male_indices, group1_constraint.male_count);
        let female_combos_g1 = combinations(&female_indices, group1_constraint.female_count);

        for male_combo in &male_combos_g1 {
            for female_combo in &female_combos_g1 {
                // Group 1 animals
                let mut group1 = male_combo.clone();
                group1.extend_from_slice(female_combo);

                // Group 2 gets the remaining animals
                let remaining_males: Vec<usize> = male_indices
                    .iter()
                    .copied()
                    .filter(|idx| !male_combo.contains(idx))
                    .collect();

                let remaining_females: Vec<usize> = female_indices
                    .iter()
                    .copied()
                    .filter(|idx| !female_combo.contains(idx))
                    .collect();

                // Validate group 2 has correct counts
                if remaining_males.len() == group2_constraint.male_count
                    && remaining_females.len() == group2_constraint.female_count
                {
                    let mut group2 = remaining_males;
                    group2.extend_from_slice(&remaining_females);

                    all_groupings.push(CandidateGrouping {
                        groups: vec![group1, group2],
                    });
                }
            }
        }
    } else {
        // For 3+ groups or complex cases, use recursive approach
        return Err(anyhow!(
            "Multi-group (>2) enumeration not yet implemented. Use 2 groups for now."
        ));
    }

    if all_groupings.is_empty() {
        return Err(anyhow!(
            "No valid groupings found. Check sex constraints and animal counts."
        ));
    }

    Ok(all_groupings)
}

/// Validate that the configuration is feasible
fn validate_config(animals: &[Animal], config: &GroupConfig) -> Result<()> {
    let male_count = animals.iter().filter(|a| a.sex == Sex::Male).count();
    let female_count = animals.iter().filter(|a| a.sex == Sex::Female).count();

    // Check total animal count matches
    let total_required: usize = config
        .sex_constraints
        .iter()
        .map(|c| c.male_count + c.female_count)
        .sum();

    if total_required != animals.len() {
        return Err(anyhow!(
            "Total animals in constraints ({}) doesn't match dataset ({})",
            total_required,
            animals.len()
        ));
    }

    // Check sex counts match
    let total_males_required: usize = config
        .sex_constraints
        .iter()
        .map(|c| c.male_count)
        .sum();

    let total_females_required: usize = config
        .sex_constraints
        .iter()
        .map(|c| c.female_count)
        .sum();

    if total_males_required != male_count {
        return Err(anyhow!(
            "Total males required ({}) doesn't match available ({})",
            total_males_required,
            male_count
        ));
    }

    if total_females_required != female_count {
        return Err(anyhow!(
            "Total females required ({}) doesn't match available ({})",
            total_females_required,
            female_count
        ));
    }

    Ok(())
}

/// Generate all combinations of choosing k items from a list
/// Returns Vec<Vec<usize>> where each inner Vec is one combination
fn combinations(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    if k == 0 {
        return vec![Vec::new()];
    }

    if k > items.len() {
        return Vec::new();
    }

    if k == items.len() {
        return vec![items.to_vec()];
    }

    let mut result = Vec::new();

    // Recursive approach
    for i in 0..=items.len() - k {
        let first = items[i];
        let rest = &items[i + 1..];
        let sub_combos = combinations(rest, k - 1);

        for mut combo in sub_combos {
            combo.insert(0, first);
            result.push(combo);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_combinations() {
        let items = vec![0, 1, 2, 3];

        // Choose 2 from 4: should be C(4,2) = 6
        let combos = combinations(&items, 2);
        assert_eq!(combos.len(), 6);
        assert!(combos.contains(&vec![0, 1]));
        assert!(combos.contains(&vec![0, 2]));
        assert!(combos.contains(&vec![0, 3]));
        assert!(combos.contains(&vec![1, 2]));
        assert!(combos.contains(&vec![1, 3]));
        assert!(combos.contains(&vec![2, 3]));

        // Choose 0 from 4: should be 1 (empty set)
        let combos = combinations(&items, 0);
        assert_eq!(combos.len(), 1);
        assert_eq!(combos[0], Vec::<usize>::new());

        // Choose 4 from 4: should be 1 (all items)
        let combos = combinations(&items, 4);
        assert_eq!(combos.len(), 1);
        assert_eq!(combos[0], items);
    }

    #[test]
    fn test_enumerate_all_simple() {
        // Create test dataset: 6 males, 4 females
        let mut animals = Vec::new();

        for i in 0..6 {
            animals.push(Animal {
                id: format!("M{}", i),
                sex: Sex::Male,
                indicators: HashMap::new(),
            });
        }

        for i in 0..4 {
            animals.push(Animal {
                id: format!("F{}", i),
                sex: Sex::Female,
                indicators: HashMap::new(),
            });
        }

        // Config: 2 groups, 5 animals each (3M+2F per group)
        let config = GroupConfig {
            num_groups: 2,
            animals_per_group: GroupSize::Uniform { value: 5 },
            sex_constraints: vec![
                SexConstraint {
                    group_index: 0,
                    male_count: 3,
                    female_count: 2,
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 3,
                    female_count: 2,
                },
            ],
        };

        let groupings = enumerate_all(&animals, &config).unwrap();

        // Expected: C(6,3) * C(4,2) = 20 * 6 = 120
        assert_eq!(groupings.len(), 120);

        // Validate first grouping
        let first = &groupings[0];
        assert_eq!(first.groups.len(), 2);
        assert_eq!(first.groups[0].len(), 5);
        assert_eq!(first.groups[1].len(), 5);

        // Validate no overlap between groups
        let group1_set: std::collections::HashSet<_> = first.groups[0].iter().collect();
        let group2_set: std::collections::HashSet<_> = first.groups[1].iter().collect();
        assert_eq!(group1_set.intersection(&group2_set).count(), 0);
    }

    #[test]
    fn test_validate_config_mismatch() {
        let animals = vec![
            Animal {
                id: "M1".to_string(),
                sex: Sex::Male,
                indicators: HashMap::new(),
            },
            Animal {
                id: "F1".to_string(),
                sex: Sex::Female,
                indicators: HashMap::new(),
            },
        ];

        // Config requires 3 males but only 1 available
        let config = GroupConfig {
            num_groups: 1,
            animals_per_group: GroupSize::Uniform { value: 2 },
            sex_constraints: vec![SexConstraint {
                group_index: 0,
                male_count: 3,
                female_count: 0,
            }],
        };

        let result = validate_config(&animals, &config);
        assert!(result.is_err());
    }
}
