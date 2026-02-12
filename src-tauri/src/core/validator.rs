use super::models::*;
use anyhow::{anyhow, Result};
use std::collections::HashSet;

pub fn validate_dataset(dataset: &Dataset) -> Result<()> {
    // Check minimum size
    if dataset.animals.len() < 4 {
        return Err(anyhow!(
            "Dataset must contain at least 4 animals (found {})",
            dataset.animals.len()
        ));
    }

    if dataset.indicator_names.is_empty() {
        return Err(anyhow!("Dataset must contain at least 1 indicator"));
    }

    // Check unique AnimalIDs
    let mut seen_ids = HashSet::new();
    for animal in &dataset.animals {
        if !seen_ids.insert(&animal.id) {
            return Err(anyhow!("Duplicate AnimalID found: {}", animal.id));
        }
    }

    // Check sex distribution
    if dataset.metadata.male_count == 0 && dataset.metadata.female_count == 0 {
        return Err(anyhow!("Dataset must contain at least one male or female"));
    }

    // Validate indicator completeness (at least 50% non-missing per animal)
    for animal in &dataset.animals {
        let present_count = animal.indicators.len();
        let total_count = dataset.indicator_names.len();
        let completeness = present_count as f64 / total_count as f64;

        if completeness < 0.5 {
            return Err(anyhow!(
                "Animal {} has too many missing values ({:.1}% present)",
                animal.id,
                completeness * 100.0
            ));
        }
    }

    Ok(())
}
