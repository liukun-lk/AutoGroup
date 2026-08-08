/**
 * LRU-style storage for recently imported Excel files.
 *
 * Entries are kept in localStorage, most recent first, deduplicated by
 * absolute file path, and capped at MAX_RECENT_IMPORTS. Only successful
 * imports should be recorded.
 */

export interface RecentImport {
  /** Absolute file path, used to re-invoke parse_excel directly. */
  path: string;
  /** File name extracted from the path, for display. */
  name: string;
  /** Last successful import time, epoch milliseconds. */
  importedAt: number;
}

const STORAGE_KEY = "autogroup.recent-imports";
export const MAX_RECENT_IMPORTS = 3;

function isValidEntry(value: unknown): value is RecentImport {
  if (typeof value !== "object" || value === null) return false;
  const entry = value as Record<string, unknown>;
  return (
    typeof entry.path === "string" &&
    entry.path.length > 0 &&
    typeof entry.name === "string" &&
    typeof entry.importedAt === "number" &&
    Number.isFinite(entry.importedAt)
  );
}

export function loadRecentImports(): RecentImport[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(isValidEntry).slice(0, MAX_RECENT_IMPORTS);
  } catch {
    return [];
  }
}

function save(entries: RecentImport[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
  } catch {
    // localStorage may be unavailable (private mode, quota); history is
    // best-effort, so failing to persist must never break the import flow.
  }
}

/**
 * Record a successful import and return the updated list.
 * Re-importing an existing path moves it to the front with a fresh timestamp.
 */
export function recordRecentImport(path: string): RecentImport[] {
  const name = path.split(/[\\/]/).pop() || path;
  const entry: RecentImport = { path, name, importedAt: Date.now() };
  const rest = loadRecentImports().filter((item) => item.path !== path);
  const updated = [entry, ...rest].slice(0, MAX_RECENT_IMPORTS);
  save(updated);
  return updated;
}

/** Remove one entry (e.g. the file no longer exists) and return the rest. */
export function removeRecentImport(path: string): RecentImport[] {
  const updated = loadRecentImports().filter((item) => item.path !== path);
  save(updated);
  return updated;
}

/** Format as "YYYY-MM-DD HH:mm:ss" in local time. */
export function formatImportTime(timestamp: number): string {
  const d = new Date(timestamp);
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  );
}
