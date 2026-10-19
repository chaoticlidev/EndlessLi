import { useEffect, useRef, useState } from "react";
import { LayoutGrid, List, PanelLeft, Plus, Search, Upload, X } from "lucide-react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { commands } from "../../ipc/bindings";
import type { LibraryItem, NewLibraryItem } from "../../ipc/bindings";
import {
  errorMessage,
  useCreateLibraryItem,
  useImportLibraryFiles,
  useLibraryItems,
} from "../../ipc/queries";
import { LibraryPanel } from "./LibraryPanel";
import { LibraryItemDetail } from "./LibraryItemDetail";
import { LIBRARY_TYPES, typeLabel } from "./libraryTypes";

function excerpt(content: string, length = 160): string {
  const plain = content
    .replace(/[#>*`[\]()\-+]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  return plain.length > length ? `${plain.slice(0, length)}…` : plain;
}

function authorsText(raw: string): string {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return parsed.filter((value) => typeof value === "string").join(", ");
    }
  } catch {
    // Not JSON: fall through to the raw value.
  }
  return raw;
}

function TypeBadge({ itemType }: { itemType: string }) {
  return (
    <span className="shrink-0 rounded border border-neutral-300 px-1.5 text-[10px] uppercase text-neutral-500 dark:border-neutral-600">
      {typeLabel(itemType)}
    </span>
  );
}

function ItemCard({
  item,
  onOpen,
}: {
  item: LibraryItem;
  onOpen: (id: string) => void;
}) {
  const large = (item.content?.length ?? 0) > 800;
  return (
    <button
      type="button"
      onClick={() => onOpen(item.id)}
      className={`flex flex-col gap-1 rounded-lg border border-neutral-300 bg-white p-3 text-left hover:border-neutral-500 dark:border-neutral-700 dark:bg-neutral-900 dark:hover:border-neutral-400 ${
        large ? "col-span-2" : ""
      }`}
    >
      <div className="flex items-center gap-2">
        <TypeBadge itemType={item.item_type} />
        {item.note_id && (
          <span className="text-[10px] uppercase text-neutral-400">Note</span>
        )}
      </div>
      <span className="truncate text-sm font-semibold">{item.title}</span>
      {item.authors && (
        <span className="truncate text-xs text-neutral-500">
          {authorsText(item.authors)}
        </span>
      )}
      {item.content && (
        <span className="line-clamp-3 text-xs text-neutral-500">
          {excerpt(item.content)}
        </span>
      )}
      <span className="mt-auto pt-1 text-[11px] text-neutral-400">
        {[item.source, item.publication_date].filter(Boolean).join(" · ")}
      </span>
    </button>
  );
}

function ItemRow({
  item,
  onOpen,
}: {
  item: LibraryItem;
  onOpen: (id: string) => void;
}) {
  return (
    <button
      type="button"
      onClick={() => onOpen(item.id)}
      className="flex w-full items-center gap-3 rounded-md border border-neutral-300 bg-white px-3 py-2 text-left text-sm hover:border-neutral-500 dark:border-neutral-700 dark:bg-neutral-900"
    >
      <TypeBadge itemType={item.item_type} />
      <span className="min-w-0 flex-1 truncate font-medium">{item.title}</span>
      <span className="shrink-0 truncate text-xs text-neutral-400">
        {item.source ?? item.publication_date ?? ""}
      </span>
    </button>
  );
}

const EMPTY_FORM = {
  itemType: "article",
  title: "",
  authors: "",
  url: "",
  source: "",
  publicationDate: "",
  content: "",
};

function NewItemDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (item: LibraryItem) => void;
}) {
  const createItem = useCreateLibraryItem();
  const [form, setForm] = useState(EMPTY_FORM);
  const [error, setError] = useState<string | null>(null);

  function set<K extends keyof typeof EMPTY_FORM>(
    key: K,
    value: (typeof EMPTY_FORM)[K],
  ) {
    setForm((prev) => ({ ...prev, [key]: value }));
  }

  async function submit() {
    if (!form.title.trim()) {
      setError("Title is required.");
      return;
    }
    setError(null);
    const input: NewLibraryItem = {
      item_type: form.itemType,
      title: form.title.trim(),
      authors: form.authors.trim()
        ? JSON.stringify([form.authors.trim()])
        : null,
      url: form.url.trim() || null,
      source: form.source.trim() || null,
      publication_date: form.publicationDate.trim() || null,
      content: form.content.trim() || null,
      file_path: null,
    };
    try {
      const item = await createItem.mutateAsync(input);
      onCreated(item);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/30 pt-20"
      onClick={onClose}
    >
      <div
        className="flex w-full max-w-lg flex-col gap-2 rounded-lg border border-neutral-300 bg-white p-4 shadow-xl dark:border-neutral-700 dark:bg-neutral-900"
        onClick={(event) => event.stopPropagation()}
      >
        <h2 className="text-sm font-semibold">New library item</h2>
        <div className="grid grid-cols-2 gap-2">
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-neutral-500">Type</span>
            <select
              value={form.itemType}
              onChange={(event) => set("itemType", event.target.value)}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm dark:border-neutral-600"
            >
              {LIBRARY_TYPES.map((type) => (
                <option key={type} value={type}>
                  {typeLabel(type)}
                </option>
              ))}
            </select>
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-neutral-500">Source</span>
            <input
              value={form.source}
              onChange={(event) => set("source", event.target.value)}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
            />
          </label>
          <label className="col-span-2 flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-neutral-500">Title</span>
            <input
              value={form.title}
              onChange={(event) => set("title", event.target.value)}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-neutral-500">Authors</span>
            <input
              value={form.authors}
              onChange={(event) => set("authors", event.target.value)}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
            />
          </label>
          <label className="flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-neutral-500">
              Publication date
            </span>
            <input
              value={form.publicationDate}
              onChange={(event) => set("publicationDate", event.target.value)}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
            />
          </label>
          <label className="col-span-2 flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-neutral-500">URL</span>
            <input
              value={form.url}
              onChange={(event) => set("url", event.target.value)}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
            />
          </label>
          <label className="col-span-2 flex flex-col gap-1 text-sm">
            <span className="text-xs font-medium text-neutral-500">Content</span>
            <textarea
              value={form.content}
              onChange={(event) => set("content", event.target.value)}
              rows={4}
              className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
            />
          </label>
        </div>
        {error && <p className="text-xs text-red-600">{error}</p>}
        <div className="flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            className="rounded-md border border-neutral-300 px-3 py-1.5 text-sm dark:border-neutral-600"
          >
            Cancel
          </button>
          <button
            type="button"
            disabled={createItem.isPending}
            onClick={() => void submit()}
            className="rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}

/** Library tab: categories, bento grid, item detail, and file import. */
export function LibraryView() {
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [listMode, setListMode] = useState(false);
  const [searchText, setSearchText] = useState("");
  const [itemType, setItemType] = useState<string | null>(null);
  const [tagId, setTagId] = useState<string | null>(null);
  const [source, setSource] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showNew, setShowNew] = useState(false);
  const [dragActive, setDragActive] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);

  const { data: items, isPending, error } = useLibraryItems({
    query: searchText,
    itemType,
    tagId,
    source,
  });
  const importFiles = useImportLibraryFiles();

  // Keep the drag-drop listener stable while always invoking the latest
  // mutation object. The ref is synced in an effect so render stays pure.
  const importRef = useRef(importFiles);
  useEffect(() => {
    importRef.current = importFiles;
  }, [importFiles]);

  function runImport(paths: string[]) {
    if (paths.length === 0) return;
    setNotice(null);
    importRef.current
      .mutateAsync(paths)
      .then((result) => {
        if (result.errors.length > 0) {
          setNotice(
            `Imported ${result.items.length}. Errors: ${result.errors.join("; ")}`,
          );
        } else {
          setNotice(`Imported ${result.items.length} file(s).`);
        }
      })
      .catch((err) => setNotice(errorMessage(err)));
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "over" || payload.type === "enter") {
          setDragActive(true);
        } else if (payload.type === "leave") {
          setDragActive(false);
        } else if (payload.type === "drop") {
          setDragActive(false);
          runImport(payload.paths);
        }
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => {
      unlisten?.();
    };
  }, []);

  async function pickAndImport() {
    setNotice(null);
    const picked = await commands.libraryPickFiles();
    if (picked.status === "error") {
      setNotice(picked.error.message);
      return;
    }
    runImport(picked.data);
  }

  return (
    <div className="relative flex h-full min-h-0 gap-3">
      {!sidebarCollapsed && (
        <aside className="w-56 shrink-0 overflow-y-auto rounded-lg border border-neutral-300 bg-white p-2 dark:border-neutral-700 dark:bg-neutral-900">
          <LibraryPanel
            itemType={itemType}
            onItemType={setItemType}
            tagId={tagId}
            onTagId={setTagId}
            source={source}
            onSource={setSource}
          />
        </aside>
      )}

      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            title={sidebarCollapsed ? "Show categories" : "Hide categories"}
            onClick={() => setSidebarCollapsed((value) => !value)}
            className="rounded-md border border-neutral-300 p-1.5 dark:border-neutral-700"
          >
            <PanelLeft size={15} />
          </button>

          <div className="flex min-w-48 flex-1 items-center gap-1.5 rounded-md border border-neutral-300 bg-white px-2 py-1.5 dark:border-neutral-700 dark:bg-neutral-900">
            <Search size={14} className="shrink-0 text-neutral-400" />
            <input
              value={searchText}
              placeholder="Search the library…"
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

          <div className="flex rounded-md border border-neutral-300 dark:border-neutral-700">
            <button
              type="button"
              title="Bento grid"
              onClick={() => setListMode(false)}
              className={`rounded-l-md p-1.5 ${!listMode ? "bg-neutral-200 dark:bg-neutral-700" : ""}`}
            >
              <LayoutGrid size={15} />
            </button>
            <button
              type="button"
              title="List"
              onClick={() => setListMode(true)}
              className={`rounded-r-md p-1.5 ${listMode ? "bg-neutral-200 dark:bg-neutral-700" : ""}`}
            >
              <List size={15} />
            </button>
          </div>

          <button
            type="button"
            onClick={() => setShowNew(true)}
            className="flex items-center gap-1.5 rounded-md border border-neutral-300 px-3 py-1.5 text-sm dark:border-neutral-700"
          >
            <Plus size={15} />
            New item
          </button>
          <button
            type="button"
            onClick={() => void pickAndImport()}
            disabled={importFiles.isPending}
            className="flex items-center gap-1.5 rounded-md bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900"
          >
            <Upload size={15} />
            {importFiles.isPending ? "Importing…" : "Import files"}
          </button>
        </div>

        {(notice || error) && (
          <p className={`text-sm ${error ? "text-red-600" : "text-neutral-500"}`}>
            {error ? errorMessage(error) : notice}
          </p>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto pb-2">
          {isPending && (
            <p className="text-sm text-neutral-500">Loading library…</p>
          )}
          {!isPending && (items ?? []).length === 0 && (
            <p className="rounded-md border border-dashed border-neutral-300 p-6 text-center text-sm text-neutral-500 dark:border-neutral-700">
              Nothing here yet. Drag files in, or use Import files.
            </p>
          )}
          {!isPending &&
            (listMode ? (
              <div className="flex flex-col gap-1.5">
                {(items ?? []).map((item) => (
                  <ItemRow key={item.id} item={item} onOpen={setSelectedId} />
                ))}
              </div>
            ) : (
              <div className="grid grid-cols-2 gap-2 xl:grid-cols-3">
                {(items ?? []).map((item) => (
                  <ItemCard key={item.id} item={item} onOpen={setSelectedId} />
                ))}
              </div>
            ))}
        </div>
      </div>

      {selectedId && (
        <aside className="w-96 shrink-0 overflow-y-auto rounded-lg border border-neutral-300 bg-white p-3 dark:border-neutral-700 dark:bg-neutral-900">
          <LibraryItemDetail
            key={selectedId}
            itemId={selectedId}
            onClose={() => setSelectedId(null)}
          />
        </aside>
      )}

      {dragActive && (
        <div className="pointer-events-none absolute inset-0 z-40 flex items-center justify-center rounded-lg border-2 border-dashed border-blue-500 bg-blue-500/10">
          <p className="rounded-md bg-white px-3 py-1.5 text-sm font-medium text-blue-700 shadow dark:bg-neutral-900 dark:text-blue-400">
            Drop files to import into the Library
          </p>
        </div>
      )}

      {showNew && (
        <NewItemDialog
          onClose={() => setShowNew(false)}
          onCreated={(item) => {
            setShowNew(false);
            setSelectedId(item.id);
          }}
        />
      )}
    </div>
  );
}
