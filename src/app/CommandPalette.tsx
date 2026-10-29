import { useMemo, useState } from "react";
import { CornerDownLeft, FilePlus, FolderPlus } from "lucide-react";
import { useUi } from "../stores/ui";
import type { TabId } from "../stores/ui";
import { useCreateNote, useCreateProject, useNoteSearch } from "../ipc/queries";
import { commands } from "../ipc/bindings";

interface PaletteAction {
  id: string;
  label: string;
  hint?: string;
  run: () => void;
}

/** Global command palette: navigation, creation, and note search. */
export function CommandPalette() {
  const paletteOpen = useUi((state) => state.paletteOpen);
  const setPaletteOpen = useUi((state) => state.setPaletteOpen);
  const setActiveTab = useUi((state) => state.setActiveTab);
  const setViewMode = useUi((state) => state.setViewMode);
  const setEditorMode = useUi((state) => state.setEditorMode);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const selectProject = useUi((state) => state.selectProject);
  const openNote = useUi((state) => state.openNote);

  const [text, setText] = useState("");
  const [index, setIndex] = useState(0);
  const [memoryModal, setMemoryModal] = useState<string[] | null>(null);
  const createNote = useCreateNote();
  const createProject = useCreateProject();
  const { data: matches } = useNoteSearch(text.trim());

  const tabs: { id: TabId; label: string }[] = useMemo(
    () => [
      { id: "knowledge", label: "Knowledge Base" },
      { id: "research", label: "Research" },
      { id: "library", label: "Library" },
      { id: "skills", label: "Skills" },
    ],
    [],
  );

  const actions: PaletteAction[] = useMemo(() => {
    const query = text.trim().toLowerCase();
    const list: PaletteAction[] = [];
    for (const tab of tabs) {
      if (!query || tab.label.toLowerCase().includes(query)) {
        list.push({
          id: `tab-${tab.id}`,
          label: `Go to ${tab.label}`,
          run: () => setActiveTab(tab.id),
        });
      }
    }
    if (!query || "new note".includes(query)) {
      list.push({
        id: "new-note",
        label: "New note in current scope",
        run: () => {
          void createNote
            .mutateAsync({
              title: "Untitled note",
              content: "",
              project_id: null,
              folder_id: null,
              author: "user",
              source_session_id: null,
              created_from_note_id: null,
            })
            .then((note) => {
              selectProject(null);
              openNote(note.id);
              setActiveTab("knowledge");
            });
        },
      });
    }
    if (!query || "new project".includes(query)) {
      list.push({
        id: "new-project",
        label: "New project…",
        hint: "prompts for a name",
        run: () => {
          const name = window.prompt("Project name:");
          if (name?.trim()) void createProject.mutateAsync(name.trim());
        },
      });
    }
    if (!query || "toggle view".includes(query)) {
      list.push({
        id: "toggle-view",
        label: "Toggle bento / list view",
        run: () =>
          setViewMode(useUi.getState().viewMode === "bento" ? "list" : "bento"),
      });
    }
    if (!query || "settings".includes(query)) {
      list.push({
        id: "open-settings",
        label: "Open settings",
        hint: "Ctrl+,",
        run: () => setSettingsOpen(true),
      });
    }
    if (!query || "preview".includes(query)) {
      list.push({
        id: "toggle-editor",
        label: "Toggle source / preview",
        run: () =>
          setEditorMode(
            useUi.getState().editorMode === "source" ? "preview" : "source",
          ),
      });
    }
    const trimmed = text.trim();
    if (trimmed.startsWith("/")) {
      const lower = trimmed.toLowerCase();
      if (lower.startsWith("/remember ")) {
        const content = trimmed.slice(10).trim();
        if (content) {
          list.push({
            id: "memory-remember",
            label: `Remember: ${content}`,
            run: () => {
              void commands.memoryCreate("global", null, content, null);
            },
          });
        }
      } else if (lower.startsWith("/project remember ")) {
        const content = trimmed.slice(18).trim();
        if (content) {
          list.push({
            id: "memory-project-remember",
            label: `Project remember: ${content}`,
            hint: "prompts for project id",
            run: () => {
              const projectId = window.prompt("Project ID:");
              if (projectId?.trim()) {
                void commands.memoryCreate("project", projectId.trim(), content, null);
              }
            },
          });
        }
      } else if (lower.startsWith("/forget ")) {
        const id = trimmed.slice(8).trim();
        if (id) {
          list.push({
            id: "memory-forget",
            label: `Forget memory ${id}`,
            run: () => {
              void commands.memoryForget(id);
            },
          });
        }
      } else if (lower === "/showmem") {
        list.push({
          id: "memory-show",
          label: "Show memories",
          run: () => {
            void commands.memoryList(null).then((result) => {
              if (result.status === "ok") {
                setMemoryModal(
                  result.data.map((m) => m.content) || ["(none)"],
                );
                setPaletteOpen(false);
              }
            });
          },
        });
      }
    }
    return list;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  const noteHits = (matches ?? []).slice(0, 6);
  const rows: (
    | { kind: "action"; action: PaletteAction }
    | { kind: "note"; id: string; title: string }
  )[] = [
    ...actions.map((action) => ({ kind: "action" as const, action })),
    ...noteHits.map((note) => ({
      kind: "note" as const,
      id: note.id,
      title: note.title,
    })),
  ];
  const clamped = Math.max(0, Math.min(index, rows.length - 1));

  function choose(row: (typeof rows)[number] | undefined) {
    if (!row) return;
    if (row.kind === "action") row.action.run();
    else {
      openNote(row.id);
      setActiveTab("knowledge");
    }
    setPaletteOpen(false);
  }

  if (memoryModal) {
    return (
      <div
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/30"
        onClick={() => setMemoryModal(null)}
      >
        <div
          className="w-full max-w-md rounded-lg border border-neutral-300 bg-white p-4 shadow-xl dark:border-neutral-700 dark:bg-neutral-900"
          onClick={(event) => event.stopPropagation()}
        >
          <h2 className="mb-2 text-sm font-semibold">Memories</h2>
          <ul className="flex flex-col gap-1">
            {memoryModal.map((content, index) => (
              <li key={index} className="text-sm">
                {content}
              </li>
            ))}
          </ul>
          <button
            type="button"
            onClick={() => setMemoryModal(null)}
            className="mt-3 rounded-md border border-neutral-300 px-2 py-1 text-sm"
          >
            Close
          </button>
        </div>
      </div>
    );
  }

  if (!paletteOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-24"
      onClick={() => setPaletteOpen(false)}
    >
      <div
        role="dialog"
        aria-label="Command palette"
        className="w-full max-w-lg overflow-hidden rounded-lg border border-border bg-card text-foreground shadow-xl"
        onClick={(event) => event.stopPropagation()}
      >
        <input
          autoFocus
          value={text}
          placeholder="Type a command or search notes…"
          onChange={(event) => {
            setText(event.target.value);
            setIndex(0);
          }}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setIndex((i) => Math.min(i + 1, rows.length - 1));
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              setIndex((i) => Math.max(i - 1, 0));
            } else if (event.key === "Enter") {
              choose(rows[clamped]);
            } else if (event.key === "Escape") {
              setPaletteOpen(false);
            }
          }}
          className="w-full border-b border-neutral-200 bg-transparent px-4 py-3 text-sm outline-none dark:border-neutral-700"
        />
        <ul className="max-h-72 overflow-y-auto p-1.5">
          {rows.length === 0 && (
            <li className="px-3 py-2 text-sm text-neutral-500">No matches.</li>
          )}
          {rows.map((row, i) => (
            <li key={row.kind === "action" ? row.action.id : `note-${row.id}`}>
              <button
                type="button"
                onMouseEnter={() => setIndex(i)}
                onClick={() => choose(row)}
                className={`flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-sm ${
                  i === clamped ? "bg-neutral-100 dark:bg-neutral-800" : ""
                }`}
              >
                {row.kind === "action" ? (
                  <>
                    {row.action.id.startsWith("new-note") ? (
                      <FilePlus
                        size={14}
                        className="shrink-0 text-neutral-400"
                      />
                    ) : row.action.id.startsWith("new-project") ? (
                      <FolderPlus
                        size={14}
                        className="shrink-0 text-neutral-400"
                      />
                    ) : (
                      <CornerDownLeft
                        size={14}
                        className="shrink-0 text-neutral-400"
                      />
                    )}
                    <span className="flex-1">{row.action.label}</span>
                    {row.action.hint && (
                      <span className="text-xs text-neutral-400">
                        {row.action.hint}
                      </span>
                    )}
                  </>
                ) : (
                  <span className="truncate">{row.title}</span>
                )}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}
