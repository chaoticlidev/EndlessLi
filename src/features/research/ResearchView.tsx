import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, History, Library, Search, Square, Trash2, X, Pencil, MoreVertical } from "lucide-react";
import { Markdown } from "../../components/Markdown";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { events, commands } from "../../ipc/bindings";
import type {
  ResearchResult,
  ResearchSession,
  ResearchStep,
} from "../../ipc/bindings";
import { Select } from "../../components/Select";
import { externalLinkProps } from "../../lib/openExternal";
import { useUi } from "../../stores/ui";
import {
  errorMessage,
  queryClient,
  useAddResearchSources,
  useCancelResearch,
  useDeleteResearch,
  useProjects,
  useResearchHistory,
  useResearchView,
  useSaveResults,
  useStartResearch,
} from "../../ipc/queries";

function dayLabel(unixTimestamp: number): string {
  const date = new Date(unixTimestamp * 1000);
  const now = new Date();

  // Zero out the time for accurate day comparisons
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const targetDate = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  
  const diffTime = today.getTime() - targetDate.getTime();
  const diffDays = Math.floor(diffTime / (1000 * 60 * 60 * 24));

  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays > 1 && diffDays < 7) return "Last 7 Days";
  if (diffDays >= 7 && diffDays < 30) return "Last 30 Days";

  // If it's older than 30 days, return the Month and Year (e.g., "September 2026")
  const formatter = new Intl.DateTimeFormat("en-US", { month: "long", year: "numeric" });
  return formatter.format(date);
}

function HistoryPanel({
  activeId,
  onSelect,
}: {
  activeId: string | null;
  onSelect: (id: string) => void;
}) {
  const { data: history } = useResearchHistory(50);
  
  // State for renaming
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [openMenuId, setOpenMenuId] = useState<string | null>(null);

  // Date grouping logic
  const groups = useMemo(() => {
    const map = new Map<string, ResearchSession[]>();
    for (const session of history ?? []) {
      const label = dayLabel(session.created_at);
      if (!map.has(label)) map.set(label, []);
      map.get(label)?.push(session);
    }
    return [...map.entries()];
  }, [history]);

  const handleRename = async (id: string) => {
    if (!editTitle.trim()) {
      setEditingId(null);
      return;
    }
    try {
      await commands.researchRename(id, editTitle);
      setEditingId(null);
      // Refresh the history list
      await queryClient.invalidateQueries({ queryKey: ["research-history"] });
    } catch (error) {
      console.error("Failed to rename", error);
    }
  };

  return (
    <aside className="flex w-60 shrink-0 flex-col gap-1 overflow-y-auto rounded-lg border border-border bg-panel/85 p-2 backdrop-blur-sm">
      <h2 className="flex items-center gap-1.5 px-1 py-1 text-xs font-semibold uppercase tracking-wide text-neutral-500">
        <History size={12} />
        History
      </h2>
      {groups.length === 0 && (
        <p className="px-1 text-xs text-neutral-500">No research yet.</p>
      )}
      
      {groups.map(([label, sessions]) => (
        <div key={label}>
          <p className="px-1 pt-1 text-[11px] font-medium text-neutral-400">
            {label}
          </p>
          
          {sessions.map((session) => (
            <div
              key={session.id}
              onClick={() => {
                if (editingId !== session.id) onSelect(session.id);
              }}
              className={`group relative flex cursor-pointer items-center justify-between rounded-md px-2 py-1 text-sm ${
                session.id === activeId
                  ? "bg-neutral-200 dark:bg-neutral-700"
                  : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
              }`}
              title={session.query}
            >
              {/* Title or Input */}
              <div className="min-w-0 flex-1 truncate pr-2">
                {editingId === session.id ? (
                  <input
                    autoFocus
                    value={editTitle}
                    onChange={(e) => setEditTitle(e.target.value)}
                    onBlur={() => void handleRename(session.id)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void handleRename(session.id);
                      if (e.key === "Escape") setEditingId(null);
                    }}
                    onClick={(e) => e.stopPropagation()}
                    className="w-full rounded border border-neutral-300 bg-white px-1 py-0.5 text-xs text-neutral-900 outline-none focus:border-neutral-500 dark:border-neutral-600 dark:bg-neutral-900 dark:text-white"
                  />
                ) : (
                  <div className="truncate">
                    <span className="mr-1.5 inline-block rounded border border-neutral-300 px-1 text-[10px] uppercase text-neutral-400 dark:border-neutral-600">
                      {session.mode}
                    </span>
                    {session.query}
                  </div>
                )}
              </div>

              {/* Three dots menu */}
              {editingId !== session.id && (
                <div className="opacity-0 transition-opacity group-hover:opacity-100">
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      setOpenMenuId(openMenuId === session.id ? null : session.id);
                    }}
                    className="rounded p-1 hover:bg-neutral-200 dark:hover:bg-neutral-700"
                  >
                    <MoreVertical size={13} />
                  </button>

                  {/* Dropdown */}
                  {openMenuId === session.id && (
                    <div className="absolute right-2 top-7 z-50 w-28 rounded-md border border-border bg-card py-1 text-foreground shadow-lg">
                      <button
                        type="button"
                        onClick={(e) => {
                          e.stopPropagation();
                          setEditingId(session.id);
                          setEditTitle(session.query);
                          setOpenMenuId(null);
                        }}
                        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-neutral-700 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"
                      >
                        <Pencil size={11} /> Rename
                      </button>
                    </div>
                  )}
                </div>
              )}
            </div>
          ))}
        </div>
      ))}
    </aside>
  );
}

function StepTree({ steps }: { steps: ResearchStep[] }) {
  const roots = steps.filter((step) => !step.parent_step_id);
  const childrenOf = (id: string) =>
    steps.filter((step) => step.parent_step_id === id);
  return (
    <ul className="flex flex-col gap-1">
      {roots.map((step) => (
        <StepNode key={step.id} step={step} depth={0} childrenOf={childrenOf} />
      ))}
    </ul>
  );
}

function StepNode({
  step,
  depth,
  childrenOf,
}: {
  step: ResearchStep;
  depth: number;
  childrenOf: (id: string) => ResearchStep[];
}) {
  const dot =
    step.status === "completed"
      ? "bg-green-600"
      : step.status === "failed"
        ? "bg-red-600"
        : "bg-amber-500 animate-pulse";
  return (
    <li>
      <div
        className="flex flex-col gap-1 text-sm"
        style={{ paddingLeft: `${depth * 16}px` }}
      >
        <div className="flex items-center gap-2">
          <span className={`h-2 w-2 shrink-0 rounded-full ${dot}`} />
          <span className={step.status === "failed" ? "text-red-600" : ""}>
            {step.description}
          </span>
          <span className="text-[11px] text-neutral-400">
            {step.status.replace("_", " ")}
          </span>
        </div>
        {step.status === "failed" && (
          <div
            className="ml-4 rounded-md border border-red-300 bg-red-50 px-2 py-1 text-xs text-red-700 dark:border-red-800 dark:bg-red-950/40 dark:text-red-300"
            style={{ marginLeft: `${depth * 16 + 16}px` }}
          >
            {step.details && step.details.trim().length > 0
              ? step.details
              : `Failed: ${step.description}`}
          </div>
        )}
      </div>
      {childrenOf(step.id).map((child) => (
        <ul key={child.id}>
          <StepNode step={child} depth={depth + 1} childrenOf={childrenOf} />
        </ul>
      ))}
    </li>
  );
}

function ResultsList({
  sessionId,
  results,
  selectable,
}: {
  sessionId: string;
  results: ResearchResult[];
  selectable: boolean;
}) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [saving, setSaving] = useState(false);
  const [title, setTitle] = useState("");
  const [projectId, setProjectId] = useState<string | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [savedNoteId, setSavedNoteId] = useState<string | null>(null);
  const [libraryNotice, setLibraryNotice] = useState<string | null>(null);

  const { data: projects } = useProjects(true);
  const saveResults = useSaveResults();
  const addSources = useAddResearchSources();
  const openNote = useUi((state) => state.openNote);
  const setActiveTab = useUi((state) => state.setActiveTab);

  const summaries = results.filter(
    (result) =>
      result.result_type === "summary" || result.result_type === "insight",
  );
  const sources = results.filter(
    (result) =>
      result.result_type === "source" || result.result_type === "citation",
  );
  const sourceIds = sources
    .filter((result) => result.result_type === "source")
    .map((result) => result.id);

  function sendToLibrary(resultIds: string[]) {
    setLibraryNotice(null);
    addSources.mutate(
      { sessionId, resultIds },
      {
        onSuccess: (items) =>
          setLibraryNotice(`Added ${items.length} item(s) to the Library.`),
        onError: (err) => setLibraryNotice(errorMessage(err)),
      },
    );
  }

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  async function save() {
    if (!title.trim() || selected.size === 0) return;
    setSaveError(null);
    try {
      const note = await saveResults.mutateAsync({
        sessionId,
        title: title.trim(),
        projectId,
        folderId: null,
        resultIds: [...selected],
      });
      setSavedNoteId(note.id);
      setSaving(false);
    } catch (err) {
      setSaveError(errorMessage(err));
    }
  }

  return (
    <div className="flex shrink-0 flex-col gap-3">
      {summaries.map((result) => (
        <article
          key={result.id}
          className="rounded-lg border border-border bg-card/80 p-3 backdrop-blur-sm"
        >
          <div className="mb-1 flex items-center gap-2">
            {selectable && (
              <input
                type="checkbox"
                aria-label="Select for note"
                checked={selected.has(result.id)}
                onChange={() => toggle(result.id)}
              />
            )}
            <span className="rounded border border-neutral-300 px-1.5 text-[11px] uppercase text-neutral-500 dark:border-neutral-600">
              {result.result_type}
            </span>
          </div>
          <div className="markdown-body text-sm">
            <Markdown content={result.content} />
          </div>
        </article>
      ))}

      {sources.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between gap-2">
            <h3 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
              Sources ({sources.length})
            </h3>
            {sourceIds.length > 0 && (
              <button
                type="button"
                disabled={addSources.isPending}
                onClick={() => sendToLibrary(sourceIds)}
                className="flex items-center gap-1 rounded-md border border-neutral-300 px-2 py-1 text-xs disabled:opacity-40 dark:border-neutral-600"
              >
                <Library size={12} />
                Add all to Library
              </button>
            )}
          </div>
          {libraryNotice && (
            <p className="text-xs text-neutral-500">{libraryNotice}</p>
          )}
          {sources.map((source) => (
            <div
              key={source.id}
              className="flex items-start gap-2 rounded-md border border-border bg-card/80 p-2"
            >
              <div className="min-w-0 flex-1">
                <a
                  {...externalLinkProps(source.source_url)}
                  className="text-sm font-medium text-blue-700 underline dark:text-blue-400"
                >
                  {source.source_title || source.source_url || "Source"}
                </a>
                {source.content && (
                  <p className="mt-0.5 line-clamp-2 text-xs text-neutral-500">
                    {source.content}
                  </p>
                )}
              </div>
              {source.result_type === "source" && (
                <button
                  type="button"
                  title="Add to Library"
                  disabled={addSources.isPending}
                  onClick={() => sendToLibrary([source.id])}
                  className="shrink-0 rounded-md border border-neutral-300 p-1.5 text-neutral-500 disabled:opacity-40 dark:border-neutral-600"
                >
                  <Library size={13} />
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      {selectable && summaries.length > 0 && (
        <div className="flex flex-col gap-2 rounded-lg border border-neutral-300 p-3 dark:border-neutral-700">
          {!saving ? (
            <button
              type="button"
              onClick={() => {
                setSaving(true);
                setSavedNoteId(null);
              }}
              disabled={selected.size === 0}
              className="w-fit rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
            >
              Save {selected.size > 0 ? `(${selected.size}) ` : ""}as note…
            </button>
          ) : (
            <>
              <input
                value={title}
                placeholder="Note title"
                onChange={(event) => setTitle(event.target.value)}
                className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
              />
              <Select
                ariaLabel="Project"
                value={projectId ?? ""}
                onChange={(value) => setProjectId(value || null)}
                options={[
                  { value: "", label: "Inbox" },
                  ...(projects ?? [])
                    .filter((project) => !project.archived)
                    .map((project) => ({
                      value: project.id,
                      label: project.name,
                    })),
                ]}
              />
              <div className="flex gap-1.5">
                <button
                  type="button"
                  onClick={() => void save()}
                  disabled={!title.trim() || selected.size === 0}
                  className="rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
                >
                  Save note
                </button>
                <button
                  type="button"
                  onClick={() => setSaving(false)}
                  className="rounded-md border border-neutral-300 px-3 py-1.5 text-sm dark:border-neutral-600"
                >
                  Cancel
                </button>
              </div>
              {saveError && <p className="text-xs text-red-600">{saveError}</p>}
              {savedNoteId && (
                <button
                  type="button"
                  onClick={() => {
                    setActiveTab("knowledge");
                    openNote(savedNoteId);
                  }}
                  className="w-fit text-xs text-blue-700 underline dark:text-blue-400"
                >
                  Open saved note
                </button>
              )}
            </>
          )}
        </div>
      )}
    </div>
  );
}

/** Research tab: query bar, live run view, and history. */
export function ResearchView() {
  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<"light" | "deep">("light");
  const [projectId, setProjectId] = useState<string | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const [startError, setStartError] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<ResearchSession | null>(
    null,
  );
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const { data: projects } = useProjects(true);
  const { data: view } = useResearchView(activeId);
  const startResearch = useStartResearch();
  const cancelResearch = useCancelResearch();
  const deleteResearch = useDeleteResearch();

  useEffect(() => {
    let alive = true;
    const unlisteners: Array<() => void> = [];
    void (async () => {
      unlisteners.push(
        await events.researchStatus.listen((event) => {
          void queryClient.invalidateQueries({
            queryKey: ["research-history"],
          });
          if (event.payload.session_id === activeId) {
            void queryClient.invalidateQueries({
              queryKey: ["research-view", activeId],
            });
          }
        }),
        await events.researchStep.listen((event) => {
          if (event.payload.session_id === activeId) {
            void queryClient.invalidateQueries({
              queryKey: ["research-view", activeId],
            });
          }
        }),
        await events.researchResult.listen((event) => {
          if (event.payload.session_id === activeId) {
            void queryClient.invalidateQueries({
              queryKey: ["research-view", activeId],
            });
          }
        }),
      );
      if (!alive) {
        for (const off of unlisteners) off();
      }
    })();
    return () => {
      alive = false;
      for (const off of unlisteners) off();
    };
  }, [activeId]);

  const session = view ?? null;
  const running =
    session?.session.status === "in_progress" ||
    session?.session.status === "pending";

  async function start() {
    const trimmed = query.trim();
    if (!trimmed) return;
    setStartError(null);
    try {
      const id = await startResearch.mutateAsync({
        query: trimmed,
        mode,
        projectId,
        folderId: null,
        sessionMemory: null,
      });
      setQuery("");
      setActiveId(id);
    } catch (err) {
      setStartError(errorMessage(err));
    }
  }

  function confirmDelete() {
    if (!pendingDelete) return;
    const sessionId = pendingDelete.id;
    setDeleteError(null);
    deleteResearch.mutate(sessionId, {
      onSuccess: () => {
        setPendingDelete(null);
        if (activeId === sessionId) setActiveId(null);
      },
      onError: (err) => setDeleteError(errorMessage(err)),
    });
  }

  return (
    <div className="flex h-full min-h-0 gap-3 overflow-hidden">
      <HistoryPanel activeId={activeId} onSelect={setActiveId} />
      
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        {/* Scrolling Area */}
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-6 pr-2">
          {!session && (
            <p className="rounded-md border border-dashed border-neutral-300 p-6 text-center text-sm text-neutral-500 dark:border-neutral-700">
              Ask below, or pick a past session from history.
            </p>
          )}

          {session && (
            <>
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  title="Back to research home"
                  onClick={() => setActiveId(null)}
                  className="rounded-md border border-neutral-300 p-1.5 dark:border-neutral-700"
                >
                  <ArrowLeft size={15} />
                </button>
                <h2 className="min-w-0 flex-1 truncate text-base font-semibold">
                  {session.session.query}
                </h2>
                <span className="rounded border border-neutral-300 px-1.5 text-[11px] uppercase text-neutral-500 dark:border-neutral-600">
                  {session.session.mode} ·{" "}
                  {session.session.status.replace("_", " ")}
                </span>
                <button
                  type="button"
                  title="Delete session"
                  aria-label="Delete session"
                  onClick={() => {
                    setDeleteError(null);
                    setPendingDelete(session.session);
                  }}
                  className="rounded-md border border-neutral-300 p-1.5 text-neutral-500 hover:text-red-600 dark:border-neutral-700"
                >
                  <Trash2 size={14} />
                </button>
                {running && (
                  <button
                    type="button"
                    onClick={() => cancelResearch.mutate(session.session.id)}
                    className="flex items-center gap-1.5 rounded-md border border-red-400 px-2.5 py-1 text-sm text-red-700 dark:text-red-400"
                  >
                    <Square size={12} />
                    Stop
                  </button>
                )}
              </div>

              <StepTree steps={session.steps} />

              <ResultsList
                sessionId={session.session.id}
                results={session.results}
                selectable={!running}
              />

              {/* This is the token counter. We add mt-4 so it sits below the summary, not inside it */}
              <p className="mt-2 text-xs text-neutral-400">
                {session.session.tokens_in + session.session.tokens_out} tokens
                this session
                {session.session.completed_at
                  ? ` · finished ${new Date(session.session.completed_at * 1000).toLocaleString()}`
                  : ""}
              </p>
            </>
          )}
        </div>

        {/* Chat Area */}
        {startError && (
          <p className="pt-2 pb-1 text-sm text-red-600">{startError}</p>
        )}

        <div className="mt-2 shrink-0 rounded-2xl border border-border bg-panel/85 p-2.5 backdrop-blur-sm">
          <div className="flex items-center gap-2">
            <Search size={15} className="shrink-0 text-neutral-400" />
            <input
              value={query}
              placeholder={
                mode === "light"
                  ? "Ask anything…"
                  : "Ask a big question for a deep dive…"
              }
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") void start();
              }}
              className="w-full bg-transparent text-sm outline-none"
            />
            {query && (
              <button
                type="button"
                aria-label="Clear query"
                onClick={() => setQuery("")}
                className="rounded-full p-1 text-neutral-400 hover:bg-neutral-100 dark:hover:bg-neutral-800"
              >
                <X size={13} />
              </button>
            )}
            <button
              type="button"
              onClick={() => void start()}
              disabled={!query.trim() || startResearch.isPending}
              className="shrink-0 rounded-xl bg-neutral-900 px-4 py-1.5 text-sm text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
            >
              {startResearch.isPending ? "Starting…" : "Send"}
            </button>
          </div>
          <div className="mt-2 flex flex-wrap items-center gap-2">
            <div className="flex rounded-full border border-neutral-300 dark:border-neutral-700">
              {(["light", "deep"] as const).map((option) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => setMode(option)}
                  className={`px-3 py-1 text-xs capitalize ${
                    option === "light" ? "rounded-l-full" : "rounded-r-full"
                  } ${mode === option ? "bg-neutral-200 dark:bg-neutral-700" : "text-neutral-500"}`}
                >
                  {option === "light" ? "Light search" : "Deep dive"}
                </button>
              ))}
            </div>
            <Select
              ariaLabel="Project"
              value={projectId ?? ""}
              onChange={(value) => setProjectId(value || null)}
              options={[
                { value: "", label: "Inbox" },
                ...(projects ?? [])
                  .filter((project) => !project.archived)
                  .map((project) => ({
                    value: project.id,
                    label: project.name,
                  })),
              ]}
              size="sm"
              pill
            />
          </div>
        </div>
      </div>

      {pendingDelete && (
        <ConfirmDialog
          title="Delete research session"
          description={
            <>
              <p>
                Delete &ldquo;{pendingDelete.query}&rdquo; with all of its
                steps and results? Notes saved from this session are kept.
              </p>
              {running && (
                <p className="mt-1">
                  The run is still in progress and will be stopped first.
                </p>
              )}
              {deleteError && (
                <p className="mt-2 text-xs text-red-600">{deleteError}</p>
              )}
            </>
          }
          busy={deleteResearch.isPending}
          onConfirm={confirmDelete}
          onCancel={() => {
            if (deleteResearch.isPending) return;
            setPendingDelete(null);
            setDeleteError(null);
          }}
        />
      )}
    </div>
  );
}
