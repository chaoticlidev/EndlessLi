import { create } from "zustand";
import type { ScriptOutcome, SkillRunOutcome } from "../ipc/bindings";

/** Stable empty object: selectors must not return a fresh `{}` each render. */
export const EMPTY_VALUES: Record<string, string> = {};

/** Confirmation shown after "Save as note" (survives remounts). */
export interface SavedNoteInfo {
  title: string;
  location: string;
  /** The output that was saved, used to block duplicate saves. */
  output: string;
}

interface WorkspaceState {
  selectedSkill: string | null;
  selectedScript: string | null;
  skillValues: Record<string, Record<string, string>>;
  scriptValues: Record<string, Record<string, string>>;
  skillOutcomes: Record<string, SkillRunOutcome>;
  scriptOutcomes: Record<string, ScriptOutcome>;
  scriptAllowNetwork: Record<string, boolean>;
  savedNotes: Record<string, SavedNoteInfo>;

  setSelectedSkill: (name: string | null) => void;
  setSelectedScript: (name: string | null) => void;
  setSkillValue: (name: string, param: string, value: string) => void;
  setScriptValue: (name: string, param: string, value: string) => void;
  setSkillOutcome: (name: string, outcome: SkillRunOutcome) => void;
  setScriptOutcome: (name: string, outcome: ScriptOutcome) => void;
  setScriptAllowNetwork: (name: string, value: boolean) => void;
  markSavedNote: (key: string, note: SavedNoteInfo) => void;
  clearSkillOutput: (name: string) => void;
  clearScriptOutput: (name: string) => void;
  clearSkillValues: (name: string) => void;
  clearScriptValues: (name: string) => void;
}

/**
 * Per-item workspace state for the Skills and Scripts tabs.
 *
 * Lives in a store (not component state) so switching tabs or clicking away
 * and back does not discard the parameter values the user typed or the last
 * run result. It is session-only: nothing here is persisted across restarts.
 */
export const useWorkspace = create<WorkspaceState>((set) => ({
  selectedSkill: null,
  selectedScript: null,
  skillValues: {},
  scriptValues: {},
  skillOutcomes: {},
  scriptOutcomes: {},
  scriptAllowNetwork: {},
  savedNotes: {},

  setSelectedSkill: (name) => set({ selectedSkill: name }),
  setSelectedScript: (name) => set({ selectedScript: name }),

  setSkillValue: (name, param, value) =>
    set((state) => ({
      skillValues: {
        ...state.skillValues,
        [name]: { ...(state.skillValues[name] ?? EMPTY_VALUES), [param]: value },
      },
    })),

  setScriptValue: (name, param, value) =>
    set((state) => ({
      scriptValues: {
        ...state.scriptValues,
        [name]: { ...(state.scriptValues[name] ?? EMPTY_VALUES), [param]: value },
      },
    })),

  setSkillOutcome: (name, outcome) =>
    set((state) => {
      const savedNotes = { ...state.savedNotes };
      delete savedNotes[`skill:${name}`];
      return {
        skillOutcomes: { ...state.skillOutcomes, [name]: outcome },
        savedNotes,
      };
    }),

  setScriptOutcome: (name, outcome) =>
    set((state) => {
      const savedNotes = { ...state.savedNotes };
      delete savedNotes[`script:${name}`];
      return {
        scriptOutcomes: { ...state.scriptOutcomes, [name]: outcome },
        savedNotes,
      };
    }),

  setScriptAllowNetwork: (name, value) =>
    set((state) => ({
      scriptAllowNetwork: { ...state.scriptAllowNetwork, [name]: value },
    })),

  markSavedNote: (key, note) =>
    set((state) => ({
      savedNotes: { ...state.savedNotes, [key]: note },
    })),

  clearSkillOutput: (name) =>
    set((state) => {
      const skillOutcomes = { ...state.skillOutcomes };
      delete skillOutcomes[name];
      const savedNotes = { ...state.savedNotes };
      delete savedNotes[`skill:${name}`];
      return { skillOutcomes, savedNotes };
    }),

  clearScriptOutput: (name) =>
    set((state) => {
      const scriptOutcomes = { ...state.scriptOutcomes };
      delete scriptOutcomes[name];
      const savedNotes = { ...state.savedNotes };
      delete savedNotes[`script:${name}`];
      return { scriptOutcomes, savedNotes };
    }),

  clearSkillValues: (name) =>
    set((state) => {
      const skillValues = { ...state.skillValues };
      delete skillValues[name];
      return { skillValues };
    }),

  clearScriptValues: (name) =>
    set((state) => {
      const scriptValues = { ...state.scriptValues };
      delete scriptValues[name];
      return { scriptValues };
    }),
}));
