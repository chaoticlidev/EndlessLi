import { useState } from "react";
import { useQueries } from "@tanstack/react-query";
import { LayoutGrid, List, Plus, Search, X } from "lucide-react";
import { useUi } from "../../stores/ui";
import { commands } from "../../ipc/bindings";
import {
  errorMessage,
  useCreateNote,
  useNoteSearch,
  useNotes,
  useTags,
} from "../../ipc/queries";
import type { Note } from "../../ipc/bindings";

function excerpt(content: string, length = 140): string {
  const plain = content
    .replace(/[#>*`[\]()\-+]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  return plain.length > length ? `${plain.slice(0, length)}…` : plain;
}

function NoteCard({ note }: { note: Note }) {
  const openNote = useUi((state) => state.openNote);
  const large = note.content.length > 800;

  return (
    <button
      type="button"
      onClick={() => openNote(note.id)}
      className={`flex flex-col gap-1 rounded-lg border border-neutral-300 bg-white p-3 text-left hover:border-neutral-500 dark:border-neutral-700 dark:bg-neutral-900 dark:hover:border-neutral-400 ${
        large ? "col-span-2" : ""
      }`}
    >
      <span className="truncate text-sm font-semibold">{note.title}</span>
      {note.content && (
        <span className="line-clamp-3 text-xs text-neutral-500">
          {excerpt(note.content)}
        </span>
      )}
      <span className="mt-auto pt-1 text-[11px] text-neutral-400">
        {new Date(note.updated_at * 1000).toLocaleDateString()}
      </span>
    </button>
  );
}

function NoteRow({ note }: { note: Note }) {
  const openNote = useUi((state) => state.openNote);
  return (
    <button
      type="button"
      onClick={() => openNote(note.id)}
      className="flex w-full items-center gap-3 rounded-md border border-neutral-300 bg-white px-3 py-2 text-left text-sm hover:border-neutral-500 dark:border-neutral-700 dark:bg-neutral-900"
    >
      <span className="min-w-0 flex-1 truncate font-medium">{note.title}</span>
      <span className="shrink-0 text-xs text-neutral-400">
        {new Date(note.updated_at * 1000).toLocaleDateString()}
      </span>
    </button>
  );
}

/** Main notes area: toolbar, tag filter, bento/list toggle, and cards. */
export function NotesView() {
  const scope = useUi((state) => state.scope)();
  const viewMode = useUi((state) => state.viewMode);
  const setViewMode = useUi((state) => state.setViewMode);
  const tagFilterId = useUi((state) => state.tagFilterId);
  const setTagFilterId = useUi((state) => state.setTagFilterId);
  const selectedProjectId = useUi((state) => state.selectedProjectId);
  const selectedFolderId = useUi((state) => state.selectedFolderId);
  const openNote = useUi((state) => state.openNote);

  const [searchText, setSearchText] = useState("");
  const searching = searchText.trim().length > 0;
  const [createError, setCreateError] = useState<string | null>(null);

  const { data: notes, isPending, error } = useNotes(scope);
  const { data: tags } = useTags();
  const { data: searchResults, isPending: searchPending } =
    useNoteSearch(searchText);
  const createNote = useCreateNote();

  const base = searching ? (searchResults ?? []) : (notes ?? []);

  // Tag filtering needs each note's tags, which live on the detail query.
  // These share the detail cache, so opening a note afterwards is instant.
  const tagDetails = useQueries({
    queries: (tagFilterId ? base : []).map((note) => ({
      queryKey: ["note-detail", note.id],
      queryFn: () =>
        commands.noteDetail(note.id).then((result) => {
          if (result.status !== "ok") throw result.error;
          return result.data;
        }),
    })),
  });

  const shown = tagFilterId
    ? base.filter((_note, i) =>
        tagDetails[i]?.data?.tags.some((tag) => tag.id === tagFilterId),
      )
    : base;
  const tagFiltering =
    tagFilterId !== null && tagDetails.some((query) => query.isPending);

  async function handleNewNote() {
    setCreateError(null);
    try {
      const created = await createNote.mutateAsync({
        title: "Untitled note",
        content: "",
        project_id: selectedFolderId ? null : selectedProjectId,
        folder_id: selectedFolderId,
        author: "user",
        source_session_id: null,
        created_from_note_id: null,
      });
      openNote(created.id);
    } catch (err) {
      setCreateError(errorMessage(err));
    }
  }

  const loading = searching ? searchPending : isPending;

  return (
    <div className="flex h-full flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex min-w-48 flex-1 items-center gap-1.5 rounded-md border border-neutral-300 bg-white px-2 py-1.5 dark:border-neutral-700 dark:bg-neutral-900">
          <Search size={14} className="shrink-0 text-neutral-400" />
          <input
            value={searchText}
            placeholder="Search notes…"
            onChange={(event) => setSearchText(event.target.value)}
            className="w-full bg-transparent text-sm outline-none"
          />
          {searchText && (
            <button
              type="button"
              aria-label="Clear search"
              onClick={() => setSearchText("")}
              className="rounded p-0.5 text-neutral-400 hover:bg-neutral-100 dark:hover:bg-neutral-800"
            >
              <X size={13} />
            </button>
          )}
        </div>

        <select
          aria-label="Filter by tag"
          value={tagFilterId ?? ""}
          onChange={(event) => setTagFilterId(event.target.value || null)}
          className="rounded-md border border-neutral-300 bg-white px-2 py-1.5 text-sm dark:border-neutral-700 dark:bg-neutral-900"
        >
          <option value="">All tags</option>
          {(tags ?? []).map((tag) => (
            <option key={tag.id} value={tag.id}>
              {tag.name}
            </option>
          ))}
        </select>

        <div className="flex rounded-md border border-neutral-300 dark:border-neutral-700">
          <button
            type="button"
            title="Bento grid"
            onClick={() => setViewMode("bento")}
            className={`rounded-l-md p-1.5 ${viewMode === "bento" ? "bg-neutral-200 dark:bg-neutral-700" : ""}`}
          >
            <LayoutGrid size={15} />
          </button>
          <button
            type="button"
            title="List"
            onClick={() => setViewMode("list")}
            className={`rounded-r-md p-1.5 ${viewMode === "list" ? "bg-neutral-200 dark:bg-neutral-700" : ""}`}
          >
            <List size={15} />
          </button>
        </div>

        <button
          type="button"
          onClick={() => void handleNewNote()}
          className="flex items-center gap-1.5 rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white dark:bg-white dark:text-neutral-900"
        >
          <Plus size={15} />
          New note
        </button>
      </div>

      {createError && <p className="text-sm text-red-600">{createError}</p>}
      {(loading || tagFiltering) && (
        <p className="text-sm text-neutral-500">Loading notes…</p>
      )}
      {error && !searching && (
        <p className="text-sm text-red-600">{errorMessage(error)}</p>
      )}

      {!loading && !tagFiltering && shown.length === 0 && (
        <p className="rounded-md border border-dashed border-neutral-300 p-6 text-center text-sm text-neutral-500 dark:border-neutral-700">
          No notes here yet. Create one to get started.
        </p>
      )}

      {viewMode === "bento" ? (
        <div className="grid grid-cols-2 gap-2 overflow-y-auto pb-2 xl:grid-cols-3">
          {shown.map((note) => (
            <NoteCard key={note.id} note={note} />
          ))}
        </div>
      ) : (
        <div className="flex flex-col gap-1.5 overflow-y-auto pb-2">
          {shown.map((note) => (
            <NoteRow key={note.id} note={note} />
          ))}
        </div>
      )}
    </div>
  );
}
