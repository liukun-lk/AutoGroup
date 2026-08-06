use super::models::*;
use anyhow::{anyhow, Result};
use std::collections::HashSet;

pub fn validate_dataset(dataset: &Dataset) -> Result<()> {
    // Check minimum size
    if dataset.animals.len() < 4 {
        return Err(anyhow!(
            "只解析到 {} 只动物，至少需要 4 只才能分组。\n\
             请检查数据行是否被空行截断，或第 1 列的动物编号是否有遗漏。",
            dataset.animals.len()
        ));
    }

    if dataset.indicator_names.is_empty() {
        return Err(anyhow!(
            "没有识别到任何指标列。请确认第 3 列及之后填写了指标名称（第 1 行英文名或单位，第 2 行中文名）。"
        ));
    }

    // Check unique AnimalIDs
    let mut seen_ids = HashSet::new();
    for animal in &dataset.animals {
        if !seen_ids.insert(&animal.id) {
            return Err(anyhow!(
                "动物编号「{}」重复出现。请确保第 1 列的动物编号唯一后重新上传。",
                animal.id
            ));
        }
    }

    // Check sex distribution
    if dataset.metadata.male_count == 0 && dataset.metadata.female_count == 0 {
        return Err(anyhow!(
            "没有识别到任何性别信息。请确认第 2 列填写了 F / M 或 雌性 / 雄性。"
        ));
    }

    // Validate indicator completeness (at least 50% non-missing per animal)
    for animal in &dataset.animals {
        let present_count = animal.indicators.len();
        let total_count = dataset.indicator_names.len();
        let completeness = present_count as f64 / total_count as f64;

        if completeness < 0.5 {
            return Err(anyhow!(
                "动物「{}」的指标缺失过多：{} 个指标中只有 {} 个有数值（{:.1}%），要求至少 50%。\n\
                 请检查该行是否存在空单元格、文本内容或公式错误（如 #DIV/0!）。",
                animal.id,
                total_count,
                present_count,
                completeness * 100.0
            ));
        }
    }

    Ok(())
}
