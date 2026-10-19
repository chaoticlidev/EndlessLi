import { useState } from "react";
import { ExternalLink, FileText, Link2, Plus, Trash2, X } from "lucide-react";
import { useUi } from "../../stores/ui";
import {
  errorMessage,
  useCreateTag,
  useDeleteLibraryItem,
  useLibraryAssignTag,
  useLibraryCreateNote,
  useLibraryDetail,
  useLibraryLinkNote,
  useLibraryUnassignTag,
  useNote,
  useProjects,
  useTitleMatches,
  useUpdateLibraryItem,
} from "../../ipc/queries";
import type { LibraryItem, UpdateLibraryItem } from "../../ipc/bindings";
import { typeLabel } from "./libraryTypes";

interface Fields {
  title: string;
  authors: string;
  url: string;
  source: string;
  publicationDate: string;
}

function fieldsFrom(item: LibraryItem): Fields {
  return {
    title: item.title,
    authors: item.authors ?? "",
    url: item.url ?? "",
    source: item.source ?? "",
    publicationDate: item.publication_date ?? "",
  };
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Right-side drawer: metadata editing, tags, note linking, and text. */
export function LibraryItemDetail({
  itemId,
  onClose,
}: {
  itemId: string;
  onClose: () => void;
}) {
  const { data: detail, isPending, error } = useLibraryDetail(itemId);
  const updateItem = useUpdateLibraryItem();
  const deleteItem = useDeleteLibraryItem();
  const assignTag = useLibraryAssignTag();
  const unassignTag = useLibraryUnassignTag();
  const createTag = useCreateTag();
  const createNote = useLibraryCreateNote();
  const linkNote = useLibraryLinkNote();

  const { data: projects } = useProjects(true);
  const linkedNote = useNote(detail?.item.note_id ?? null);

  const openNote = useUi((state) => state.openNote);
  const setActiveTab = useUi((state) => state.setActiveTab);

  const [fields, setFields] = useState<Fields | null>(null);
  const [saved, setSaved] = useState<Fields | null>(null);
  const [tagDraft, setTagDraft] = useState("");
  const [noteQuery, setNoteQuery] = useState("");
  const [projectId, setProjectId] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const { data: matches } = useTitleMatches(noteQuery);

  // Adopt server state once, when the drawer first loads the item.
  if (detail && fields === null) {
    const next = fieldsFrom(detail.item);
    setFields(next);
    setSaved(next);
  }

  if (isPending) {
    return <p className="text-sm text-neutral-500">Loading item…</p>;
  }
  if (error) {
    return <p className="text-sm text-red-600">{errorMessage(error)}</p>;
  }
  if (!detail) {
    return <p className="text-sm text-neutral-500">Item not found.</p>;
  }

  const item = detail.item;
  const dirty =
    fields !== null &&
    saved !== null &&
    (fields.title !== saved.title ||
      fields.authors !== saved.authors ||
      fields.url !== saved.url ||
      fields.source !== saved.source ||
      fields.publicationDate !== saved.publicationDate);

  function update<K extends keyof Fields>(key: K, value: string) {
    setFields((prev) => (prev ? { ...prev, [key]: value } : prev));
  }

  async function save() {
    if (!fields) return;
    setActionError(null);
    setMessage(null);
    const patch: UpdateLibraryItem = {
      title: fields.title !== item.title ? fields.title : null,
      authors: fields.authors !== (item.authors ?? "") ? fields.authors : null,
      url: fields.url !== (item.url ?? "") ? fields.url : null,
      source: fields.source !== (item.source ?? "") ? fields.source : null,
      publication_date:
        fields.publicationDate !== (item.publication_date ?? "")
          ? fields.publicationDate
          : null,
      content: null,
      file_path: null,
      note_id: null,
    };
    try {
      await updateItem.mutateAsync({ id: item.id, patch });
      setSaved(fields);
      setMessage("Saved");
    } catch (err) {
      setActionError(errorMessage(err));
    }
  }

  async function addTag() {
    const name = tagDraft.trim();
    if (!name) return;
    setActionError(null);
    try {
      const tag = await createTag.mutateAsync(name);
      await assignTag.mutateAsync({ itemId: item.id, tagId: tag.id });
      setTagDraft("");
    } catch (err) {
      setActionError(errorMessage(err));
    }
  }

  return (
    <div className="flex h-full flex-col gap-3 overflow-y-auto">
      <div className="flex items-center gap-2">
        <span className="rounded border border-neutral-300 px-1.5 py-0.5 text-[11px] uppercase text-neutral-500 dark:border-neutral-600">
          {typeLabel(item.item_type)}
        </span>
        <span className="flex-1" />
        <button
          type="button"
          aria-label="Close item"
          onClick={onClose}
          className="rounded-md border border-neutral-300 p-1.5 dark:border-neutral-700"
        >
          <X size={14} />
        </button>
      </div>

      <label className="flex flex-col gap-1 text-sm">
        <span className="text-xs font-medium text-neutral-500">Title</span>
        <input
          value={fields?.title ?? ""}
          onChange={(event) => update("title", event.target.value)}
          className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
        />
      </label>

      <div className="grid grid-cols-2 gap-2">
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-xs font-medium text-neutral-500">Authors</span>
          <input
            value={fields?.authors ?? ""}
            onChange={(event) => update("authors", event.target.value)}
            className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-xs font-medium text-neutral-500">Source</span>
          <input
            value={fields?.source ?? ""}
            onChange={(event) => update("source", event.target.value)}
            className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-xs font-medium text-neutral-500">URL</span>
          <input
            value={fields?.url ?? ""}
            onChange={(event) => update("url", event.target.value)}
            className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
          />
        </label>
        <label className="flex flex-col gap-1 text-sm">
          <span className="text-xs font-medium text-neutral-500">
            Publication date
          </span>
          <input
            value={fields?.publicationDate ?? ""}
            onChange={(event) => update("publicationDate", event.target.value)}
            className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
          />
        </label>
      </div>

      <div className="flex items-center gap-2">
        <button
          type="button"
          disabled={!dirty || updateItem.isPending}
          onClick={() => void save()}
          className="rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
        >
          Save changes
        </button>
        {message && (
          <span className="text-xs text-green-700 dark:text-green-400">
            {message}
          </span>
        )}
        {item.url && (
          <a
            href={item.url}
            target="_blank"
            rel="noreferrer"
            className="flex items-center gap-1 text-xs text-blue-700 underline dark:text-blue-400"
          >
            <ExternalLink size={12} />
            Open source
          </a>
        )}
      </div>

      {item.file_path && (
        <p className="flex items-center gap-1 text-xs text-neutral-500">
          <FileText size={12} />
          Managed copy: {fileName(item.file_path)}
        </p>
      )}

      <div className="flex flex-col gap-1.5 border-t border-neutral-200 pt-2 dark:border-neutral-700">
        <span className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Tags
        </span>
        <div className="flex flex-wrap items-center gap-1.5">
          {detail.tags.map((tag) => (
            <span
              key={tag.id}
              className="flex items-center gap-1 rounded-full border border-neutral-300 px-2 py-0.5 text-xs dark:border-neutral-600"
            >
              {tag.name}
              <button
                type="button"
                aria-label={`Remove tag ${tag.name}`}
                onClick={() =>
                  unassignTag.mutate({ itemId: item.id, tagId: tag.id })
                }
                className="rounded-full text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
              >
                ×
              </button>
            </span>
          ))}
          <input
            value={tagDraft}
            placeholder="Add tag…"
            onChange={(event) => setTagDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void addTag();
            }}
            className="w-28 rounded-full border border-dashed border-neutral-300 bg-transparent px-2 py-0.5 text-xs outline-none dark:border-neutral-600"
          />
        </div>
      </div>

      <div className="flex flex-col gap-1.5 border-t border-neutral-200 pt-2 dark:border-neutral-700">
        <span className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Note
        </span>
        {item.note_id ? (
          <div className="flex items-center gap-2 text-sm">
            <button
              type="button"
              onClick={() => {
                setActiveTab("knowledge");
                openNote(item.note_id);
              }}
              className="min-w-0 flex-1 truncate text-left text-blue-700 underline dark:text-blue-400"
            >
              {linkedNote.data?.title ?? "Open linked note"}
            </button>
            <button
              type="button"
              onClick={() => linkNote.mutate({ itemId: item.id, noteId: null })}
              className="shrink-0 rounded-md border border-neutral-300 px-2 py-1 text-xs dark:border-neutral-600"
            >
              Unlink
            </button>
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            <div className="flex items-center gap-1.5">
              <select
                aria-label="Project for new note"
                value={projectId ?? ""}
                onChange={(event) => setProjectId(event.target.value || null)}
                className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-xs dark:border-neutral-600"
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
              <button
                type="button"
                onClick={() =>
                  createNote.mutate(
                    { itemId: item.id, projectId, folderId: null },
                    {
                      onSuccess: (note) => {
                        setActiveTab("knowledge");
                        openNote(note.id);
                      },
                      onError: (err) => setActionError(errorMessage(err)),
                    },
                  )
                }
                className="flex items-center gap-1 rounded-md bg-neutral-900 px-2.5 py-1 text-xs text-white dark:bg-white dark:text-neutral-900"
              >
                <Plus size={12} />
                Create note
              </button>
            </div>
            <div className="flex items-center gap-1.5">
              <Link2 size={13} className="shrink-0 text-neutral-400" />
              <input
                value={noteQuery}
                placeholder="Link an existing note…"
                onChange={(event) => setNoteQuery(event.target.value)}
                className="w-full rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-xs outline-none dark:border-neutral-600"
              />
            </div>
            {noteQuery.trim() && (
              <div className="flex flex-col gap-0.5">
                {(matches ?? []).map((note) => (
                  <button
                    key={note.id}
                    type="button"
                    onClick={() =>
                      linkNote.mutate(
                        { itemId: item.id, noteId: note.id },
                        { onSuccess: () => setNoteQuery("") },
                      )
                    }
                    className="truncate rounded px-2 py-1 text-left text-xs hover:bg-neutral-100 dark:hover:bg-neutral-800"
                  >
                    {note.title}
                  </button>
                ))}
                {(matches ?? []).length === 0 && (
                  <p className="px-1 text-xs text-neutral-500">
                    No matching notes.
                  </p>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {item.content && (
        <div className="flex flex-col gap-1.5 border-t border-neutral-200 pt-2 dark:border-neutral-700">
          <span className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
            Extracted text
          </span>
          <pre className="max-h-72 overflow-y-auto whitespace-pre-wrap rounded-md border border-neutral-200 bg-neutral-50 p-2 text-xs dark:border-neutral-700 dark:bg-neutral-950">
            {item.content}
          </pre>
        </div>
      )}

      {actionError && <p className="text-xs text-red-600">{actionError}</p>}

      <div className="mt-auto border-t border-neutral-200 pt-2 dark:border-neutral-700">
        <button
          type="button"
          onClick={() => {
            if (window.confirm("Delete this item and its managed copy?")) {
              deleteItem.mutate(item.id, {
                onSuccess: onClose,
                onError: (err) => setActionError(errorMessage(err)),
              });
            }
          }}
          className="flex items-center gap-1 rounded-md border border-red-300 px-2.5 py-1 text-xs text-red-700 dark:border-red-800 dark:text-red-400"
        >
          <Trash2 size={12} />
          Delete item
        </button>
      </div>
    </div>
  );
}
