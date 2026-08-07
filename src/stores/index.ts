import { atom } from "jotai";
import type {
  Dataset,
  GroupConfig,
  StatConfig,
  GroupingResult,
  AppStep,
} from "../types";

// App state atoms
export const currentStepAtom = atom<AppStep>("upload");
export const datasetAtom = atom<Dataset | null>(null);
export const groupConfigAtom = atom<GroupConfig | null>(null);
export const statConfigAtom = atom<StatConfig | null>(null);
/** One computation run: every candidate it produced and which one is on display. */
export interface GroupingRun {
  candidates: GroupingResult[];
  selectedIndex: number;
  totalEvaluated: number;
  totalValid: number;
}

export const groupingRunAtom = atom<GroupingRun | null>(null);

/** The candidate currently on display; a read-only view over the run. */
export const resultAtom = atom<GroupingResult | null>((get) => {
  const run = get(groupingRunAtom);
  return run ? (run.candidates[run.selectedIndex] ?? null) : null;
});
export const isLoadingAtom = atom<boolean>(false);
export const errorAtom = atom<string | null>(null);

// Derived atoms
export const hasDatasetAtom = atom((get) => get(datasetAtom) !== null);
export const hasResultAtom = atom((get) => get(groupingRunAtom) !== null);

// Selected indicators atom (for configuration)
export const selectedIndicatorsAtom = atom<string[]>([]);

// Progress tracking
export const canProceedToConfigureAtom = atom((get) => {
  const dataset = get(datasetAtom);
  return dataset !== null && dataset.animals.length > 0;
});

export const canProceedToComputeAtom = atom((get) => {
  const groupConfig = get(groupConfigAtom);
  const statConfig = get(statConfigAtom);
  return groupConfig !== null && statConfig !== null;
});

// Actions (write-only atoms)
export const resetStateAtom = atom(null, (_get, set) => {
  set(currentStepAtom, "upload");
  set(datasetAtom, null);
  set(groupConfigAtom, null);
  set(statConfigAtom, null);
  set(groupingRunAtom, null);
  set(errorAtom, null);
  set(selectedIndicatorsAtom, []);
});

export const setErrorAtom = atom(null, (_get, set, message: string) => {
  set(errorAtom, message);
  set(isLoadingAtom, false);
});

export const clearErrorAtom = atom(null, (_get, set) => {
  set(errorAtom, null);
});
