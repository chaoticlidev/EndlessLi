import { PanelLeft } from "lucide-react";
import { useUi } from "../stores/ui";
import { ProjectsPanel } from "../features/knowledge-base/ProjectsPanel";
import { NotesView } from "../features/knowledge-base/NotesView";
import { NoteEditor } from "../features/knowledge-base/NoteEditor";

/** Knowledge Base tab: collapsible projects panel plus notes or editor. */
export function KnowledgeBaseView() {
  const sidebarCollapsed = useUi((state) => state.sidebarCollapsed);
  const toggleSidebar = useUi((state) => state.toggleSidebar);
  const openNoteId = useUi((state) => state.openNoteId);

  return (
    <div className="flex h-full min-h-0 gap-3">
      {!sidebarCollapsed && (
        <aside className="w-64 shrink-0 overflow-y-auto rounded-lg border border-border bg-panel p-2 thin-scroll">
          <ProjectsPanel />
        </aside>
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex items-center gap-2">
          <button
            type="button"
            title={sidebarCollapsed ? "Show sidebar" : "Hide sidebar"}
            onClick={toggleSidebar}
            className="rounded-md border border-border p-1.5"
          >
            <PanelLeft size={15} />
          </button>
        </div>
        <div className="min-h-0 flex-1">
          {openNoteId ? (
            <NoteEditor key={openNoteId} noteId={openNoteId} />
          ) : (
            <NotesView />
          )}
        </div>
      </div>
    </div>
  );
}