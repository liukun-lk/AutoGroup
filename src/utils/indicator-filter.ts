/**
 * Utility functions for filtering indicators based on naming patterns
 */

/**
 * Patterns to match ID-like and name-like fields that should be excluded from statistical analysis
 */
const EXCLUDED_INDICATOR_PATTERNS = [
  // ID-related patterns (case-insensitive)
  /^sample.*id/i,
  /^sample.*no/i,
  /^animal.*id/i,
  /^animal.*no/i,
  /样本号/,
  /样品识别号/,
  /编号/,

  // Name-related patterns (case-insensitive)
  /^name$/i,
  /^full.*name$/i,
  /^animal.*name$/i,
  /名称$/,
  /姓名$/,
];

/**
 * Determines if an indicator should be excluded from default selection
 * based on naming patterns (ID fields, name fields, etc.)
 *
 * @param indicatorName - The name of the indicator to check
 * @returns true if the indicator should be excluded, false otherwise
 */
export function shouldExcludeIndicator(indicatorName: string): boolean {
  const trimmedName = indicatorName.trim();

  return EXCLUDED_INDICATOR_PATTERNS.some(pattern =>
    pattern.test(trimmedName)
  );
}

/**
 * Filters a list of indicators to exclude ID/name-like fields
 *
 * @param indicators - Array of indicator names
 * @returns Filtered array with ID/name fields removed
 */
export function filterDefaultIndicators(indicators: string[]): string[] {
  return indicators.filter(indicator => !shouldExcludeIndicator(indicator));
}

/**
 * Gets a list of excluded indicators for display purposes
 *
 * @param indicators - Array of all indicator names
 * @returns Array of indicator names that were excluded
 */
export function getExcludedIndicators(indicators: string[]): string[] {
  return indicators.filter(indicator => shouldExcludeIndicator(indicator));
}
