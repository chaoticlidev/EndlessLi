import { useEffect, useMemo, useState } from "react";
import { ArrowLeft, History, Search, Square, X } from "lucide-react";
import { events } from "../../ipc/bindings";
import type {
  ResearchResult,
  ResearchSession,
  ResearchStep,
} from "../../ipc/bindings";
import { useUi } from "../../stores/ui";
import {
  errorMessage,
  queryClient,
  useCancelResearch,
  useProjects,
  useResearchHistory,
  useResearchView,
  useSaveResults,
  useStartResearch,
} from "../../ipc/queries";

function dayLabel(epochSecs: number): string {
  const date = new Date(epochSecs * 1000);
  const today = new Date();
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (date.toDateString() === today.toDateString()) return "Today";
  if (date.toDateString() === yesterday.toDateString()) return "Yesterday";
  return date.toLocaleDateString();
}

function HistoryPanel({
  activeId,
  onSelect,
}: {
  activeId: string | null;
  onSelect: (id: string) => void;
}) {
  const { data: history } = useResearchHistory(50);
  const groups = useMemo(() => {
    const map = new Map<string, ResearchSession[]>();
    for (const session of history ?? []) {
      const label = dayLabel(session.created_at);
      if (!map.has(label)) map.set(label, []);
      map.get(label)?.push(session);
    }
    return [...map.entries()];
  }, [history]);

  return (
    <aside className="flex w-60 shrink-0 flex-col gap-1 overflow-y-auto rounded-lg border border-neutral-300 bg-white p-2 dark:border-neutral-700 dark:bg-neutral-900">
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
            <button
              key={session.id}
              type="button"
              onClick={() => onSelect(session.id)}
              className={`block w-full truncate rounded-md px-2 py-1 text-left text-sm ${
                session.id === activeId
                  ? "bg-neutral-200 dark:bg-neutral-700"
                  : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
              }`}
              title={session.query}
            >
              <span className="mr-1.5 inline-block rounded border border-neutral-300 px-1 text-[10px] uppercase text-neutral-400 dark:border-neutral-600">
                {session.mode}
              </span>
              {session.query}
            </button>
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
        className="flex items-center gap-2 text-sm"
        style={{ paddingLeft: `${depth * 16}px` }}
      >
        <span className={`h-2 w-2 shrink-0 rounded-full ${dot}`} />
        <span className={step.status === "failed" ? "text-red-600" : ""}>
          {step.description}
        </span>
        <span className="text-[11px] text-neutral-400">
          {step.status.replace("_", " ")}
        </span>
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

  const { data: projects } = useProjects(true);
  const saveResults = useSaveResults();
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
    <div className="flex flex-col gap-2">
      {summaries.map((result) => (
        <article
          key={result.id}
          className="rounded-lg border border-neutral-300 bg-white p-3 dark:border-neutral-700 dark:bg-neutral-900"
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
          <p className="whitespace-pre-wrap text-sm">{result.content}</p>
        </article>
      ))}

      {sources.length > 0 && (
        <div className="flex flex-col gap-1.5">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
            Sources ({sources.length})
          </h3>
          {sources.map((source) => (
            <div
              key={source.id}
              className="rounded-md border border-neutral-200 bg-white p-2 dark:border-neutral-700 dark:bg-neutral-900"
            >
              <a
                href={source.source_url ?? undefined}
                target="_blank"
                rel="noreferrer"
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
              <select
                aria-label="Project"
                value={projectId ?? ""}
                onChange={(event) => setProjectId(event.target.value || null)}
                className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm dark:border-neutral-600"
              >
                <option value="">Inbox</option>
                {(projects ?? [])
                  .filter((project) => !project.archived)
                  .map((project) => (
                    <option key={project.id} value={project.id}>
                      {project.name}
                    </option>
                  ))}
              </select>
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

  const { data: projects } = useProjects(true);
  const { data: view } = useResearchView(activeId);
  const startResearch = useStartResearch();
  const cancelResearch = useCancelResearch();

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

  return (
    <div className="flex h-full min-h-0 gap-3">
      <HistoryPanel activeId={activeId} onSelect={setActiveId} />
      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-2">
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

              <p className="text-xs text-neutral-400">
                {session.session.tokens_in + session.session.tokens_out} tokens
                this session
                {session.session.completed_at
                  ? ` · finished ${new Date(session.session.completed_at * 1000).toLocaleString()}`
                  : ""}
              </p>
            </>
          )}
        </div>

        {startError && (
          <p className="pb-1 text-sm text-red-600">{startError}</p>
        )}

        <div className="rounded-2xl border border-neutral-300 bg-white p-2.5 dark:border-neutral-700 dark:bg-neutral-900">
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
            <select
              aria-label="Project"
              value={projectId ?? ""}
              onChange={(event) => setProjectId(event.target.value || null)}
              className="rounded-full border border-neutral-300 bg-transparent px-2.5 py-1 text-xs dark:border-neutral-700"
            >
              <option value="">Inbox</option>
              {(projects ?? [])
                .filter((project) => !project.archived)
                .map((project) => (
                  <option key={project.id} value={project.id}>
                    {project.name}
                  </option>
                ))}
            </select>
          </div>
        </div>
      </div>
    </div>
  );
}
