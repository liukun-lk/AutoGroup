# Test Data Format Specification

## 1. Original Excel Structure

### File: `通用动物实验自动分组软件_测试用数据.xlsx`

```
Row 1 (Headers):    [None, None, 'kg', '℃', 'ALT', 'AST', 'TP', 'ALB', ...]
Row 2 (Labels):     ['动物编号', '性别', '体重', '肛温', 'U/L', 'U/L', 'g/L', ...]
Row 3 (Data):       ['XHP2601001', 'F', 31.85, 38.5, 58.8, 23.1, 76.6, ...]
Row 4 (Data):       ['XHP2601002', 'F', 30.45, 38.5, 42.2, 17.2, 75.4, ...]
...
Row 11 (Data):      ['XHP2601009', 'M', 33.5, 38.1, 31.1, 20.7, 70.2, ...]
```

### Key Observations

**Dataset Size:**
- Total rows: 11 (1 header + 1 label row + 10 data rows including 1 unit row)
- Total columns: 75
- Actual animals: 10
- Actual indicators: 73 (excluding AnimalID and Sex columns)

**Animal Distribution:**
- Males: 6 animals (XHP2601004-009)
- Females: 4 animals (XHP2601001-003, plus one more)

**Data Structure Issues:**
1. **Multi-row headers**: Row 1 has English indicator names, Row 2 has Chinese labels + units
2. **Mixed content in Row 2**: Contains both column descriptions and unit information
3. **Empty cells**: Row 1 has `None` for first 2 columns (AnimalID, Sex)

---

## 2. Parsing Strategy

### 2.1 Header Parsing

**Row 1 (Indicator Names):**
- Skip first 2 columns (AnimalID, Sex)
- Read remaining columns as indicator names
- Filter out `None` or empty values

**Row 2 (Skip):**
- This row contains Chinese labels and units
- Not used for data parsing (metadata only)

### 2.2 Data Row Parsing

**Starting from Row 3:**
- Column 0: AnimalID (String, e.g., `XHP2601001`)
- Column 1: Sex (String, `M` or `F`)
- Columns 2+: Numeric indicator values (Float64)

**Data Validation:**
- AnimalID must be unique and non-empty
- Sex must be `M` or `F` (case-insensitive)
- Indicator values should be numeric (handle `None` as missing data)

---

## 3. Expected Grouping Scenario

### Test Case 1: Balanced 2-Group Split

**Configuration:**
- Number of groups: 2
- Animals per group: 5
- Sex constraint: 3 Males + 2 Females per group

**Input:**
- 10 animals total (6M, 4F)
- 73 indicators

**Expected Grouping:**
```
Group 1:
  - 3 males from {XHP2601004, XHP2601005, XHP2601006, XHP2601007, XHP2601008, XHP2601009}
  - 2 females from {XHP2601001, XHP2601002, XHP2601003, ???}

Group 2:
  - Remaining 3 males
  - Remaining 2 females
```

**Note:** Need to verify actual female count in test data (4 or different?)

**Statistical Test:**
- Method: Independent t-test (2 groups)
- For each of 73 indicators:
  - Levene test for variance homogeneity
  - If homogeneous: Student t-test
  - If not: Welch t-test
- Optimization: Find grouping where max(min(P)) is maximized

---

## 4. Data Format Specification for Parser

### 4.1 Input Excel Requirements

**Mandatory Structure:**
```
Row 1: [ID_header, Sex_header, Indicator1_name, Indicator2_name, ...]
Row 2+: [animal_id, sex_value, indicator1_value, indicator2_value, ...]
```

**Flexible Parsing Rules:**
1. **Auto-detect header row**: First row with non-numeric values in columns 3+
2. **Auto-detect ID column**: First column with unique string values
3. **Auto-detect Sex column**: First column with only `M`/`F` values
4. **Skip empty rows**: Ignore rows where all cells are empty
5. **Handle missing data**: Allow `None`/`NaN` in indicator columns

### 4.2 Validation Rules

**Dataset-level:**
- Minimum 4 animals (for meaningful statistics)
- At least 2 indicators
- At least 1 male and 1 female (for sex-balanced grouping)

**Animal-level:**
- Unique AnimalID
- Valid Sex value (`M`, `F`, `Male`, `Female`, case-insensitive)
- At least 50% of indicators have non-missing values

**Indicator-level:**
- At least 80% of animals have non-missing values for this indicator
- Values should be numeric (coerce to Float64)

---

## 5. Example Parsed Dataset (JSON)

```json
{
  "animals": [
    {
      "id": "XHP2601001",
      "sex": "Female",
      "indicators": {
        "kg": 31.85,
        "℃": 38.5,
        "ALT": 58.8,
        "AST": 23.1,
        "TP": 76.6,
        "ALB": 36.2,
        ...
      }
    },
    {
      "id": "XHP2601002",
      "sex": "Female",
      "indicators": {
        "kg": 30.45,
        "℃": 38.5,
        "ALT": 42.2,
        ...
      }
    },
    ...
  ],
  "indicator_names": ["kg", "℃", "ALT", "AST", "TP", "ALB", ...],
  "metadata": {
    "total_animals": 10,
    "male_count": 6,
    "female_count": 4,
    "indicator_count": 73
  }
}
```

---

## 6. Edge Cases to Handle

### 6.1 Excel Format Variations

**Case 1: Single-row header**
```
Row 1: ['AnimalID', 'Sex', 'Indicator1', 'Indicator2', ...]
Row 2+: Data
```
**Solution:** Detect if Row 2 contains data (not labels/units)

**Case 2: Multi-language headers**
```
Row 1: English names
Row 2: Chinese names
Row 3: Units
Row 4+: Data
```
**Solution:** Skip rows until numeric data is found

**Case 3: Extra metadata columns**
```
Columns: [AnimalID, Sex, Group(existing), Weight, Height, Indicator1, ...]
```
**Solution:** User manually selects which columns are indicators

### 6.2 Data Quality Issues

**Missing values:**
- Option 1: Exclude animal if > 50% missing
- Option 2: Exclude indicator if > 20% missing
- Option 3: Imputation (mean/median) - NOT recommended for grouping

**Outliers:**
- Do NOT remove outliers automatically
- Provide visual warnings in data preview

**Duplicated AnimalIDs:**
- Error: Reject file and show which IDs are duplicated

---

## 7. UI Preview Component Requirements

After parsing, display:

**Summary Card:**
```
✓ Successfully imported 10 animals
  - Males: 6 (60%)
  - Females: 4 (40%)
  - Indicators: 73
  - Missing data: 2.5%
```

**Data Table (first 10 rows):**
| AnimalID    | Sex | kg    | ℃    | ALT  | AST  | ... |
|-------------|-----|-------|------|------|------|-----|
| XHP2601001  | F   | 31.85 | 38.5 | 58.8 | 23.1 | ... |
| XHP2601002  | F   | 30.45 | 38.5 | 42.2 | 17.2 | ... |
| ...         |     |       |      |      |      |     |

**Indicator Selector (Checkbox List):**
```
☑ kg (Body weight)
☑ ℃ (Temperature)
☑ ALT (Alanine aminotransferase)
☑ AST (Aspartate aminotransferase)
...
Select All | Deselect All | Select Common (~20 indicators)
```

---

## 8. Recommended Test Scenarios

### Test 1: Basic 2-Group Split
- 10 animals → 2 groups × 5 animals
- Sex: 3M+2F per group
- All 73 indicators
- Mode: Strict
- Expected: Should find valid grouping with all P > 0.05

### Test 2: Unbalanced Groups
- 10 animals → Group1(6) + Group2(4)
- Sex: Any valid distribution
- Selected indicators: 10 common ones
- Mode: Optimized
- Expected: Allow 1 indicator with P ≤ 0.05

### Test 3: 3-Group Split (if extend test data)
- Would need at least 15 animals
- 3 groups × 5 animals
- ANOVA + post-hoc tests

---

## 9. Appendix: Full Indicator List (from test data)

```
Column 3+:
kg, ℃, ALT, AST, TP, ALB, TBIL, ALP, GGT, GLU, UREA, CREA, Ca, P,
CHOl, TG, CK, LDH, K, Na, CL, GLOB, A/G, AST/ALT,
[50+ more columns including blood cell counts, coagulation parameters, etc.]
```

**Suggested grouping:**
- Basic biochemistry: ALT, AST, TP, ALB, TBIL, ALP, GGT, GLU, UREA, CREA
- Electrolytes: Ca, P, K, Na, CL
- Lipids: CHOl, TG
- Enzymes: CK, LDH
- Ratios: A/G, AST/ALT

Total common indicators for quick testing: ~20
