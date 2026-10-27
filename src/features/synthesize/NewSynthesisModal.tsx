import { useState, useEffect, useMemo } from "react";
import {
  FileText,
  Plus,
  Search,
  Upload,
  X,
} from "lucide-react";
import { commands } from "../../ipc/bindings";
import type { LibraryItem, Note } from "../../ipc/bindings";
import {
  errorMessage,
  useLibraryItems,
  useNotes,
  useProjects,
  useStartSynthesis,
} from "../../ipc/queries";
import { Select } from "../../components/Select";
import type { SelectOption } from "../../components/Select";

interface DocumentEntry {
  doc_ref: string;
  text: string;
  title: string;
}

const MEDIA_TYPE_OPTIONS: SelectOption[] = [
  { value: "report", label: "Report" },
  { value: "slides", label: "Slides" },
  { value: "flashcards", label: "Flashcards" },
  { value: "quiz", label: "Quiz" },
  { value: "audio", label: "Audio script" },
  { value: "dataset", label: "Dataset" },
];

const LIBRARY_TYPE_OPTIONS: SelectOption[] = [
  { value: "", label: "All types" },
  { value: "article", label: "Article" },
  { value: "pdf", label: "PDF" },
  { value: "book", label: "Book" },
  { value: "video", label: "Video" },
  { value: "podcast", label: "Podcast" },
  { value: "dataset", label: "Dataset" },
];

function SourceChip({
  docRef,
  title,
  onRemove,
}: {
  docRef: string;
  title: string;
  onRemove: () => void;
}) {
  return (
    <div
      className="flex items-center gap-1 rounded-md border border-neutral-300 bg-white px-2 py-1 text-xs text-neutral-700 dark:border-neutral-600 dark:bg-neutral-900"
      title={docRef}
    >
      <FileText size={10} className="shrink-0" />
      <span className="max-w-[160px] truncate">{title || docRef}</span>
      <button
        type="button"
        onClick={onRemove}
        className="rounded p-0.5 text-neutral-400 hover:text-red-600"
      >
        <X size={10} />
      </button>
    </div>
  );
}

function NotePicker({
  onAdd,
}: {
  onAdd: (doc: DocumentEntry) => void;
}) {
  // Fetch ALL notes, regardless of what folder you are clicked into
  const { data: notes, isPending } = useNotes({ type: "all" });
  const [searchText, setSearchText] = useState("");

  const filtered = useMemo(() => {
    const trimmed = searchText.trim().toLowerCase();
    if (!trimmed) return notes ?? [];
    return (notes ?? []).filter(
      (note) =>
        note.title.toLowerCase().includes(trimmed) ||
        note.content.toLowerCase().includes(trimmed),
    );
  }, [notes, searchText]);

  if (isPending) {
    return <p className="text-xs text-neutral-500">Loading notes…</p>;
  }

  if (filtered.length === 0) {
    return <p className="text-xs text-neutral-500">No matching notes.</p>;
  }

  return (
    <div className="flex flex-col gap-1 max-h-44 overflow-y-auto">
      <div className="relative">
        <Search size={12} className="absolute left-2 top-1.5 text-neutral-400" />
        <input
          value={searchText}
          placeholder="Search notes…"
          onChange={(e) => setSearchText(e.target.value)}
          className="w-full rounded-md border border-neutral-300 bg-transparent pl-7 pr-2 py-1 text-xs outline-none dark:border-neutral-600"
        />
      </div>
      {filtered.map((note: Note) => (
        <button
          key={note.id}
          type="button"
          onClick={() =>
            onAdd({
              doc_ref: `note:${note.id}`,
              text: `${note.title}\n\n${note.content}`,
              title: note.title,
            })
          }
          className="flex items-start gap-2 rounded-md border border-neutral-200 p-2 text-left text-xs hover:border-neutral-400 dark:border-neutral-700 dark:hover:border-neutral-600"
        >
          <Plus size={11} className="mt-0.5 shrink-0 text-neutral-500" />
          <span className="min-w-0 flex-1 truncate">{note.title}</span>
        </button>
      ))}
    </div>
  );
}

function LibraryPicker({
  onAdd,
}: {
  onAdd: (doc: DocumentEntry) => void;
}) {
  const [query, setQuery] = useState("");
  const [itemType, setItemType] = useState<string | null>(null);
  const { data: items, isPending } = useLibraryItems({
    query,
    itemType,
    tagId: null,
    source: null,
  });

  const shown = useMemo(() => {
    if (isPending) return [];
    return items ?? [];
  }, [items, isPending]);

  return (
    <div className="flex flex-col gap-2">
      <div className="flex gap-1.5">
        <div className="relative flex-1">
          <Search size={12} className="absolute left-2 top-1.5 text-neutral-400" />
          <input
            value={query}
            placeholder="Search library…"
            onChange={(e) => setQuery(e.target.value)}
            className="w-full rounded-md border border-neutral-300 bg-transparent pl-7 pr-2 py-1 text-xs outline-none dark:border-neutral-600"
          />
        </div>
        <Select
          ariaLabel="Type filter"
          value={itemType ?? ""}
          onChange={(value) => setItemType(value || null)}
          options={LIBRARY_TYPE_OPTIONS}
          size="sm"
        />
      </div>

      {isPending && <p className="text-xs text-neutral-500">Loading…</p>}
      {!isPending && shown.length === 0 && (
        <p className="text-xs text-neutral-500">No library items match.</p>
      )}

      <div className="flex flex-col gap-1 max-h-44 overflow-y-auto">
        {shown.map((item: LibraryItem) => {
          const hasContent = item.content && item.content.trim().length > 0;
          return (
            <button
              key={item.id}
              type="button"
              onClick={() =>
                hasContent
                  ? onAdd({
                      doc_ref: `lib:${item.id}`,
                      text: item.content ?? "",
                      title: item.title,
                    })
                  : void 0
              }
              disabled={!hasContent}
              className="flex items-start gap-2 rounded-md border border-neutral-200 p-2 text-left text-xs hover:border-neutral-400 disabled:opacity-40 dark:border-neutral-700 dark:hover:border-neutral-600"
              title={hasContent ? item.title : `${item.title} (no content)`}
            >
              <Plus size={11} className="mt-0.5 shrink-0 text-neutral-500" />
              <span className="min-w-0 flex-1 truncate">{item.title}</span>
              {item.source && (
                <span className="shrink-0 text-neutral-400">[{item.source}]</span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}

function ManualSource({
  onAdd,
}: {
  onAdd: (doc: DocumentEntry) => void;
}) {
  const [text, setText] = useState("");
  const [title, setTitle] = useState("");

  function add() {
    if (!text.trim()) return;
    const ref = `manual:${Date.now()}`;
    onAdd({
      doc_ref: ref,
      text: text.trim(),
      title: title.trim() || `Manual source (${ref})`,
    });
    setText("");
    setTitle("");
  }

  return (
    <div className="flex flex-col gap-2">
      <input
        value={title}
        placeholder="Source title (optional)"
        onChange={(e) => setTitle(e.target.value)}
        className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-xs outline-none dark:border-neutral-600"
      />
      <textarea
        value={text}
        placeholder="Paste text content here…"
        onChange={(e) => setText(e.target.value)}
        rows={6}
        className="rounded-md border border-neutral-300 bg-transparent px-2 py-1.5 text-xs outline-none dark:border-neutral-600"
      />
      <button
        type="button"
        onClick={add}
        disabled={!text.trim()}
        className="w-fit rounded-md bg-neutral-900 px-3 py-1.5 text-xs text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
      >
        Add as document
      </button>
    </div>
  );
}

const SOURCE_TABS = ["notes", "library", "manual"] as const;

type SourceTab = (typeof SOURCE_TABS)[number];

export function NewSynthesisModal({
  onClose,
  onStart,
}: {
  onClose: () => void;
  onStart: (jobId: string) => void;
}) {
  const [title, setTitle] = useState("");
  const [mediaType, setMediaType] = useState("report");
  const [kMode, setKMode] = useState<"auto" | "manual">("auto");
  const [kValue, setKValue] = useState("8");
  const [maxContext] = useState("");
  const [instructions, setInstructions] = useState("");
  const [projectId, setProjectId] = useState<string | null>(null);
  const [sourceTab, setSourceTab] = useState<SourceTab>("notes");
  const [documents, setDocuments] = useState<DocumentEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [importNotice, setImportNotice] = useState<string | null>(null);

  const { data: projects } = useProjects(true);
  const startSynthesis = useStartSynthesis();

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onClose]);

  function addDocument(doc: DocumentEntry) {
    if (documents.some((d) => d.doc_ref === doc.doc_ref)) return;
    setDocuments((prev) => [...prev, doc]);
  }

  function removeDocument(docRef: string) {
    setDocuments((prev) => prev.filter((d) => d.doc_ref !== docRef));
  }

  async function pickAndImport() {
    setImportNotice(null);
    try {
      const picked = await commands.libraryPickFiles();
      if (picked.status === "error") {
        setImportNotice(picked.error.message);
        return;
      }
      if (picked.data.length === 0) return;
      const result = await commands.libraryImportFiles(picked.data);
      if (result.status === "ok") {
        if (result.data.errors.length > 0) {
          setImportNotice(`Imported ${result.data.items.length}. Errors: ${result.data.errors.join("; ")}`);
        } else {
          setImportNotice(`Imported ${result.data.items.length} file(s).`);
        }
      } else {
        setImportNotice(result.error.message);
      }
    } catch (err) {
      setImportNotice(errorMessage(err));
    }
  }

  function switchToLibraryTab() {
    setSourceTab("library");
  }

  async function start() {
    if (!title.trim()) {
      setError("A title is required.");
      return;
    }
    if (documents.length === 0) {
      setError("Add at least one document.");
      return;
    }
    setError(null);
    try {
      const jobId = await startSynthesis.mutateAsync({
        title: title.trim(),
        documents: documents.map((doc) => ({
          doc_ref: doc.doc_ref,
          text: doc.text,
        })),
        k: kMode === "manual" ? parseInt(kValue, 10) : null,
        max_context_tokens: maxContext ? parseInt(maxContext, 10) : 8192,
        media_type: mediaType,
        instructions: instructions.trim(),
        project_id: projectId,
        folder_id: null,
      });
      onStart(jobId);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  const canStart = documents.length > 0;

    const getKLabel = () => {
    switch (mediaType) {
      case "report": return "How many distinct reports?";
      case "audio": return "How many distinct audio scripts?";
      case "slides": return "How many slides in the deck?";
      case "flashcards": return "How many flashcards?";
      case "quiz": return "How many quiz questions?";
      case "dataset": return "How many rows in the dataset?";
      default: return "How many outputs?";
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4"
      onClick={onClose}
    >
      <div
        className="flex w-full max-w-3xl flex-col gap-4 rounded-lg border border-neutral-300 bg-white p-4 shadow-xl dark:border-neutral-700 dark:bg-neutral-900"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className="text-sm font-semibold">New synthesis</h2>

        {error && <p className="text-xs text-red-600">{error}</p>}

        <div className="grid grid-cols-2 gap-4">
          <div className="flex flex-col gap-1 text-sm">
            <label className="text-xs font-medium text-neutral-500">Title</label>
            <input
              value={title}
              placeholder="e.g. Q4 market analysis"
              onChange={(e) => setTitle(e.target.value)}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
            />
          </div>

          <div className="flex flex-col gap-1 text-sm">
            <label className="text-xs font-medium text-neutral-500">Media type</label>
            <Select
              ariaLabel="Media type"
              value={mediaType}
              onChange={setMediaType}
              options={MEDIA_TYPE_OPTIONS}
              size="sm"
            />
          </div>

          <div className="flex flex-col gap-1 text-sm">
            <label className="text-xs font-medium text-neutral-500">
              {getKLabel()}
            </label>
            <div className="flex items-center gap-1.5">
              <label className="flex items-center gap-1 text-xs">
                <input
                  type="radio"
                  name="kMode"
                  checked={kMode === "auto"}
                  onChange={() => setKMode("auto")}
                />
                Auto
              </label>
              <label className="flex items-center gap-1 text-xs">
                <input
                  type="radio"
                  name="kMode"
                  checked={kMode === "manual"}
                  onChange={() => setKMode("manual")}
                />
                Manual
              </label>
            </div>
            {kMode === "manual" && (
              <input
                type="number"
                min={1}
                max={1000}
                value={kValue}
                onChange={(e) => setKValue(e.target.value)}
                className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
              />
            )}
          </div>

          <div className="col-span-2 flex flex-col gap-1 text-sm">
            <label className="text-xs font-medium text-neutral-500">
              Project (for saved note)
            </label>
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
          </div>

          <div className="col-span-2 flex flex-col gap-1 text-sm">
            <label className="text-xs font-medium text-neutral-500">
              Instructions
            </label>
            <textarea
              value={instructions}
              placeholder="Special instructions for the synthesis…"
              onChange={(e) => setInstructions(e.target.value)}
              rows={3}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1.5 text-sm outline-none dark:border-neutral-600"
            />
          </div>

          <div className="col-span-2 flex flex-col gap-1 text-sm">
            <label className="text-xs font-medium text-neutral-500">
              Documents
            </label>
            <div className="flex gap-1.5">
              {SOURCE_TABS.map((tab) => (
                <button
                  key={tab}
                  type="button"
                  onClick={() => setSourceTab(tab)}
                  className={`px-2.5 py-1 text-xs capitalize ${
                    sourceTab === tab
                      ? "rounded-md bg-neutral-900 text-white dark:bg-white dark:text-neutral-900"
                      : "rounded-md border border-neutral-300 text-neutral-600 dark:border-neutral-700"
                  }`}
                >
                  {tab}
                </button>
              ))}
            </div>
          </div>

          {importNotice && (
            <p className="col-span-2 text-xs text-neutral-500">{importNotice}</p>
          )}

          <div className="col-span-2">
            {sourceTab === "notes" && (
              <>
                <NotePicker onAdd={addDocument} />
                <NoteLibraryNote onSwitch={switchToLibraryTab} />
              </>
            )}
            {sourceTab === "library" && <LibraryPicker onAdd={addDocument} />}
            {sourceTab === "manual" && <ManualSource onAdd={addDocument} />}
          </div>

          {documents.length > 0 && (
            <div className="col-span-2 flex flex-wrap gap-1">
              {documents.map((doc) => (
                <SourceChip
                  key={doc.doc_ref}
                  docRef={doc.doc_ref}
                  title={doc.title}
                  onRemove={() => removeDocument(doc.doc_ref)}
                />
              ))}
            </div>
          )}

          <div className="col-span-2 flex justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-md border border-neutral-300 px-3 py-1.5 text-sm dark:border-neutral-700"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => void pickAndImport()}
              className="flex items-center gap-1 rounded-md border border-neutral-300 px-3 py-1.5 text-sm dark:border-neutral-700"
            >
              <Upload size={13} />
              Import files
            </button>
            <button
              type="button"
              onClick={() => void start()}
              disabled={!canStart || startSynthesis.isPending}
              className="flex items-center gap-1.5 rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
            >
              {startSynthesis.isPending ? "Starting…" : "Synthesize"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

function NoteLibraryNote({ onSwitch }: { onSwitch: () => void }) {
  return (
    <div className="pt-2 text-center">
      <button
        type="button"
        onClick={onSwitch}
        className="text-xs text-blue-700 underline dark:text-blue-400"
      >
        Need files? Import first, then select from Library.
      </button>
    </div>
  );
}
