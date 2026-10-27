import { useEffect, useState } from "react";
import {
  History,
  Plus,
  Square,
  Save,
  FileText,
  Trash2,
  ArrowLeft,
} from "lucide-react";
import { Markdown } from "../../components/Markdown";
import { ConfirmDialog } from "../../components/ConfirmDialog";
import { events } from "../../ipc/bindings";
import type {
  SynthesisJob,
  SynthesisProgressEvent,
} from "../../ipc/bindings";
import {
  errorMessage,
  queryClient,
  useDeleteSynthesis,
  useProjects,
  useSynthesisHistory,
  useSynthesisJob,
  useSaveSynthesisOutput,
  useCancelSynthesis,
} from "../../ipc/queries";
import { useUi } from "../../stores/ui";
import { NewSynthesisModal } from "./NewSynthesisModal";
import { Select } from "../../components/Select";

function dayLabel(unixTimestamp: number): string {
  const date = new Date(unixTimestamp * 1000);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const targetDate = new Date(date.getFullYear(), date.getMonth(), date.getDate());
  const diffDays = Math.floor((today.getTime() - targetDate.getTime()) / 86400000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  if (diffDays > 1 && diffDays < 7) return "Last 7 Days";
  if (diffDays >= 7 && diffDays < 30) return "Last 30 Days";
  return new Intl.DateTimeFormat("en-US", { month: "long", year: "numeric" }).format(date);
}

function mediaLabel(mediaType: string): string {
  const labels: Record<string, string> = {
    report: "Report",
    slides: "Slides",
    flashcards: "Flashcards",
    quiz: "Quiz",
    audio: "Audio script",
    dataset: "Dataset",
  };
  return labels[mediaType] ?? mediaType;
}

function statusLabel(status: string | undefined): string {
  if (!status) return "Unknown";
  const labels: Record<string, string> = {
    pending: "Pending",
    in_progress: "In progress",
    completed: "Completed",
    failed: "Failed",
    cancelled: "Cancelled",
  };
  return labels[status] ?? status.replace("_", " ");
}

function HistoryPanel({
  activeId,
  onSelect,
}: {
  activeId: string | null;
  onSelect: (id: string) => void;
}) {
  const { data: history } = useSynthesisHistory(50);

  const groups = (() => {
    const map = new Map<string, SynthesisJob[]>();
    for (const job of history ?? []) {
      const label = dayLabel(job.created_at);
      if (!map.has(label)) map.set(label, []);
      map.get(label)?.push(job);
    }
    return [...map.entries()] as [string, SynthesisJob[]][];
  })();

  return (
    <aside className="flex w-64 shrink-0 flex-col gap-1 overflow-y-auto rounded-lg border border-neutral-300 bg-white p-2 dark:border-neutral-700 dark:bg-neutral-900">
      <div className="flex items-center justify-between px-1 py-1">
        <h2 className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-neutral-500">
          <History size={12} />
          History
        </h2>
      </div>
      {groups.length === 0 && (
        <p className="px-1 text-xs text-neutral-500">No syntheses yet.</p>
      )}
      {groups.map(([label, jobs]) => (
        <div key={label}>
          <p className="px-1 pt-1 text-[11px] font-medium text-neutral-400">{label}</p>
          {jobs.map((job) => {
            const isActive = job.id === activeId;
            const isRunning = job.status === "pending" || job.status === "in_progress";
            const iconColor =
              job.status === "failed"
                ? "text-red-500"
                : isRunning
                  ? "text-amber-500 animate-pulse"
                  : "text-green-600";
            return (
              <div
                key={job.id}
                onClick={() => onSelect(job.id)}
                className={`group relative flex cursor-pointer items-center justify-between rounded-md px-2 py-1.5 text-sm ${
                  isActive
                    ? "bg-neutral-200 dark:bg-neutral-700"
                    : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
                }`}
                title={job.title}
              >
                <div className="min-w-0 flex-1 truncate pr-2">
                  <div className="truncate">
                    <span
                      className={`mr-1.5 inline-block rounded border border-neutral-300 px-1 text-[10px] uppercase text-neutral-400 dark:border-neutral-600 ${iconColor}`}
                    >
                      {mediaLabel(job.media_type)}
                    </span>
                    {job.title}
                  </div>
                </div>
                {isRunning && (
                  <span className="shrink-0 text-[10px] text-neutral-400">
                    {statusLabel(job.status)}
                  </span>
                )}
                {!isRunning && job.status === "failed" && (
                  <span className="shrink-0 text-[10px] text-red-600">
                    {statusLabel(job.status)}
                  </span>
                )}
                {!isRunning && (job.status === "completed" || job.status === "cancelled") && (
                  <span className="shrink-0 text-[10px] text-neutral-400">
                    {statusLabel(job.status)}
                  </span>
                )}
              </div>
            );
          })}
        </div>
      ))}
    </aside>
  );
}

function ProgressBar({ progress }: { progress: number }) {
  const clamped = Math.max(0, Math.min(1, progress));
  const width = Math.round(clamped * 100);
  return (
    <div className="h-1.5 w-full rounded-full bg-neutral-200 dark:bg-neutral-700">
      <div
        className="h-full rounded-full bg-blue-600 transition-all"
        style={{ width: `${width}%` }}
      />
    </div>
  );
}

function OutputView({ jobId, content }: { jobId: string; content: string }) {
  const saveOutput = useSaveSynthesisOutput();
  const { data: projects } = useProjects(true);
  const setActiveTab = useUi((state) => state.setActiveTab);
  const openNote = useUi((state) => state.openNote);

  const [saveTitle, setSaveTitle] = useState("");
  const [projectId, setProjectId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [savedNoteId, setSavedNoteId] = useState<string | null>(null);

  async function save() {
    if (!saveTitle.trim()) return;
    setSaveError(null);
    try {
      const note = await saveOutput.mutateAsync({
        id: jobId,
        title: saveTitle,
        projectId,
        folderId: null,
      });
      setSavedNoteId(note.id);
    } catch (err) {
      setSaveError(errorMessage(err));
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="rounded-lg border border-neutral-300 bg-white p-3 dark:border-neutral-700 dark:bg-neutral-900">
        <div className="markdown-body">
          <Markdown content={content} />
        </div>
      </div>

      <div className="flex flex-col gap-2 rounded-lg border border-neutral-300 p-3 dark:border-neutral-700">
        {!saving ? (
          <button
            type="button"
            onClick={() => setSaving(true)}
            className="w-fit rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white dark:bg-white dark:text-neutral-900"
          >
            <Save size={14} className="mr-1.5 inline" />
            Save as note…
          </button>
        ) : (
          <>
            <input
              value={saveTitle}
              placeholder="Note title"
              onChange={(e) => setSaveTitle(e.target.value)}
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
                  .map((project) => ({ value: project.id, label: project.name })),
              ]}
              size="sm"
            />
            <div className="flex gap-1.5">
              <button
                type="button"
                onClick={() => void save()}
                disabled={!saveTitle.trim() || saveOutput.isPending}
                className="rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
              >
                {saveOutput.isPending ? "Saving…" : "Save note"}
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
    </div>
  );
}

function ActiveRunView({
  jobId,
  onDelete,
  onBack,
}: {
  jobId: string;
  onDelete: (job: SynthesisJob) => void;
  onBack: () => void; // <-- Add this prop type
}) {
  const { data: job } = useSynthesisJob(jobId);
  const cancelSynthesis = useCancelSynthesis();
  const [isCanceling, setIsCanceling] = useState(false);
  const [progress, setProgress] = useState<SynthesisProgressEvent | null>(null);

  useEffect(() => {
    let alive = true;

    void events.synthesisProgress.listen((event) => {
      if (!alive) return;
      setProgress(event.payload);
      void queryClient.invalidateQueries({ queryKey: ["synthesis-job", jobId] });
    });

    return () => {
      alive = false;
    };
  }, [jobId]);

  useEffect(() => {
    if (job?.status === "failed" || job?.status === "cancelled" || job?.status === "completed") {
      setIsCanceling(false);
    }
  }, [job?.status]);

  const running = job?.status === "pending" || job?.status === "in_progress";

  if (!job) {
    return (
      <div className="flex min-h-0 flex-1 items-center justify-center">
        <p className="text-sm text-neutral-500">Loading…</p>
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto pb-4">
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onBack}
          className="rounded-md border border-neutral-300 p-1.5 text-neutral-900 hover:bg-neutral-100 dark:border-neutral-700 dark:text-white dark:hover:bg-neutral-800 transition-colors"
          title="Back to workspace"
        >
          <ArrowLeft size={18} />
        </button>

        <h2 className="min-w-0 flex-1 truncate text-base font-semibold">
          {job.title}
        </h2>
        <span className="rounded border border-neutral-300 px-1.5 text-[11px] uppercase text-neutral-500 dark:border-neutral-600">
          {isCanceling ? "Canceling..." : statusLabel(job.status)}
        </span>
        {running && (
          <button
            type="button"
            onClick={() => {
            setIsCanceling(true);
            cancelSynthesis.mutate(jobId);
          }}
            className="flex items-center gap-1 rounded-md border border-red-400 px-2.5 py-1 text-sm text-red-700 dark:text-red-400"
          >
            <Square size={12} />
            Stop
          </button>
        )}
        {running && (
          <button
            type="button"
            disabled={isCanceling}
            onClick={() => {
              setIsCanceling(true);
              cancelSynthesis.mutate(jobId);
            }}
            className="flex items-center gap-1 rounded-md border border-red-400 px-2.5 py-1 text-sm text-red-700 disabled:opacity-50 dark:text-red-400"
          >
            <Square size={12} />
            {isCanceling ? "Canceling..." : "Stop"}
          </button>
        )}
      </div>

      <div className="flex flex-col gap-1.5 rounded-lg border border-neutral-300 bg-white p-3 dark:border-neutral-700 dark:bg-neutral-900">
        <div className="flex items-center justify-between gap-2">
          <span className="text-xs text-neutral-500">
            {progress?.stage ?? (running ? "Starting" : "Finished")}
          </span>
          <span className="text-xs text-neutral-400">
            {Math.round((progress?.progress ?? (running ? 0 : 1)) * 100)}%
          </span>
        </div>
        <ProgressBar progress={progress?.progress ?? (running ? 0 : 1)} />
        {progress && (
          <p className="text-xs text-neutral-600 dark:text-neutral-300">
            {progress.message}
          </p>
        )}
      </div>

      {job.output && <OutputView jobId={jobId} content={job.output} />}

      {!running && job.status === "failed" && job.error_message && (
        <div className="rounded-md border border-red-300 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-800 dark:bg-red-950/40 dark:text-red-300">
          {job.error_message}
        </div>
      )}
    </div>
  );
}

/** Synthesize tab: run status, history, and output display. */
export function SynthesizeView() {
  const [activeId, setActiveId] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<SynthesisJob | null>(null);
  const [deleteError, setDeleteError] = useState<string | null>(null);

  const { data: activeJob } = useSynthesisJob(activeId);
  const deleteSynthesis = useDeleteSynthesis();

  useEffect(() => {
    if (activeId) {
      void queryClient.invalidateQueries({ queryKey: ["synthesis-job", activeId] });
    }
  }, [activeId]);

  function confirmDelete() {
    if (!pendingDelete) return;
    const jobId = pendingDelete.id;
    setDeleteError(null);
    deleteSynthesis.mutate(jobId, {
      onSuccess: () => {
        setPendingDelete(null);
        if (activeId === jobId) setActiveId(null);
        void queryClient.invalidateQueries({ queryKey: ["synthesis-history"] });
      },
      onError: (err) => setDeleteError(errorMessage(err)),
    });
  }

  return (
    <div className="flex h-full min-h-0 gap-3 overflow-hidden">
      <HistoryPanel activeId={activeId} onSelect={setActiveId} />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col">
        {!activeJob && !showNew && (
          <div className="flex min-h-0 flex-1 items-center justify-center">
            <div className="flex flex-col items-center gap-3 text-center">
              <FileText size={48} className="text-neutral-300 dark:text-neutral-600" />
              <p className="text-sm text-neutral-500">
                No synthesis selected. Create one to get started.
              </p>
              <button
                type="button"
                onClick={() => setShowNew(true)}
                className="flex items-center gap-1.5 rounded-md bg-neutral-900 px-4 py-2 text-sm text-white dark:bg-white dark:text-neutral-900"
              >
                <Plus size={15} />
                New synthesis
              </button>
            </div>
          </div>
        )}

        {activeJob && (
          <ActiveRunView
            jobId={activeJob.id}
            onDelete={(job) => setPendingDelete(job)}
            onBack={() => setActiveId(null)}
          />
        )}
      </div>

      {showNew && (
        <NewSynthesisModal
          onClose={() => setShowNew(false)}
          onStart={(jobId) => {
            setShowNew(false);
            setActiveId(jobId);
          }}
        />
      )}

      {pendingDelete && (
        <ConfirmDialog
          title="Delete synthesis job"
          description={
            <>
              <p>
                Delete &ldquo;{pendingDelete.title}&rdquo;? The saved output
                (if any) is not removed.
              </p>
              {deleteError && (
                <p className="mt-2 text-xs text-red-600">{deleteError}</p>
              )}
            </>
          }
          busy={deleteSynthesis.isPending}
          onConfirm={confirmDelete}
          onCancel={() => {
            if (deleteSynthesis.isPending) return;
            setPendingDelete(null);
            setDeleteError(null);
          }}
        />
      )}
    </div>
  );
}
