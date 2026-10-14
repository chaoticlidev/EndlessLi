import { create } from "zustand";
import type { NoteScope } from "../ipc/bindings";

export type TabId =
  "knowledge" | "research" | "library" | "skills" | "settings";
export type ViewMode = "bento" | "list";
export type EditorMode = "source" | "preview";

interface UiState {
  activeTab: TabId;
  setActiveTab: (tab: TabId) => void;

  /** Selected project id, or null for the Inbox. */
  selectedProjectId: string | null;
  selectProject: (id: string | null) => void;

  /** Selected folder id within the project, or null for the whole scope. */
  selectedFolderId: string | null;
  selectFolder: (id: string | null) => void;

  /** Open note id, or null when no editor is open. */
  openNoteId: string | null;
  openNote: (id: string | null) => void;

  viewMode: ViewMode;
  setViewMode: (mode: ViewMode) => void;

  editorMode: EditorMode;
  setEditorMode: (mode: EditorMode) => void;

  /** Tag id used to filter the note list, or null for no filter. */
  tagFilterId: string | null;
  setTagFilterId: (id: string | null) => void;

  paletteOpen: boolean;
  setPaletteOpen: (open: boolean) => void;
  /** Bumped on every open so the palette remounts with fresh state. */
  paletteNonce: number;

  sidebarCollapsed: boolean;
  toggleSidebar: () => void;

  /** The listing scope derived from the current selection. */
  scope: () => NoteScope;
}

export const useUi = create<UiState>((set, get) => ({
  activeTab: "knowledge",
  setActiveTab: (activeTab) => set({ activeTab }),

  selectedProjectId: null,
  selectProject: (selectedProjectId) =>
    set({ selectedProjectId, selectedFolderId: null }),

  selectedFolderId: null,
  selectFolder: (selectedFolderId) => set({ selectedFolderId }),

  openNoteId: null,
  openNote: (openNoteId) => set({ openNoteId }),

  viewMode: "bento",
  setViewMode: (viewMode) => set({ viewMode }),

  editorMode: "source",
  setEditorMode: (editorMode) => set({ editorMode }),

  tagFilterId: null,
  setTagFilterId: (tagFilterId) => set({ tagFilterId }),

  paletteOpen: false,
  paletteNonce: 0,
  setPaletteOpen: (paletteOpen) =>
    set((state) => ({
      paletteOpen,
      paletteNonce: paletteOpen ? state.paletteNonce + 1 : state.paletteNonce,
    })),

  sidebarCollapsed: false,
  toggleSidebar: () =>
    set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),

  scope: () => {
    const { selectedProjectId, selectedFolderId } = get();
    if (selectedFolderId)
      return { type: "folder", folder_id: selectedFolderId };
    if (selectedProjectId)
      return { type: "project", project_id: selectedProjectId };
    return { type: "inbox" };
  },
}));
