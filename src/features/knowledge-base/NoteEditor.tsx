import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import { ArrowLeft, Eye, GitBranch, Pencil, Save, Trash2 } from "lucide-react";
import { useUi } from "../../stores/ui";
import { CodeMirrorEditor } from "../../components/CodeMirrorEditor";
import { LinksPanel } from "./LinksPanel";
import { renderWikilinksAsLinks, wikilinkTitleFromHref } from "./wikilinks";
import type { FolderNode, NoteDetail } from "../../ipc/bindings";
import {
  errorMessage,
  useAssignTag,
  useBranchNote,
  useCreateTag,
  useDeleteNote,
  useFolderTree,
  useMoveNote,
  useNoteDetail,
  useProjects,
  useTags,
  useTitleMatches,
  useUnassignTag,
  useUpdateNote,
} from "../../ipc/queries";

function TagRow({ noteId }: { noteId: string }) {
  const { data: detail } = useNoteDetail(noteId);
  const { data: allTags } = useTags();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const createTag = useCreateTag();
  const assignTag = useAssignTag();
  const unassignTag = useUnassignTag();

  async function addTag() {
    const name = draft.trim();
    if (!name) return;
    setError(null);
    try {
      const tag = await createTag.mutateAsync(name);
      await assignTag.mutateAsync({ noteId, tagId: tag.id });
      setDraft("");
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  const attachedIds = new Set((detail?.tags ?? []).map((tag) => tag.id));
  const suggestions = (allTags ?? [])
    .filter((tag) => !attachedIds.has(tag.id))
    .slice(0, 8);

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-1.5">
        {(detail?.tags ?? []).map((tag) => (
          <span
            key={tag.id}
            className="flex items-center gap-1 rounded-full border border-neutral-300 px-2 py-0.5 text-xs dark:border-neutral-600"
          >
            {tag.name}
            <button
              type="button"
              aria-label={`Remove tag ${tag.name}`}
              onClick={() => unassignTag.mutate({ noteId, tagId: tag.id })}
              className="rounded-full text-neutral-400 hover:text-neutral-700 dark:hover:text-neutral-200"
            >
              ×
            </button>
          </span>
        ))}
        <input
          value={draft}
          placeholder="Add tag…"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void addTag();
          }}
          className="w-28 rounded-full border border-dashed border-neutral-300 bg-transparent px-2 py-0.5 text-xs outline-none dark:border-neutral-600"
        />
      </div>
      {draft.trim() && suggestions.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {suggestions
            .filter((tag) =>
              tag.name.toLowerCase().includes(draft.trim().toLowerCase()),
            )
            .map((tag) => (
              <button
                key={tag.id}
                type="button"
                onClick={() => {
                  assignTag.mutate(
                    { noteId, tagId: tag.id },
                    {
                      onSuccess: () => setDraft(""),
                      onError: (err) => setError(errorMessage(err)),
                    },
                  );
                }}
                className="rounded-full bg-neutral-100 px-2 py-0.5 text-xs hover:bg-neutral-200 dark:bg-neutral-800 dark:hover:bg-neutral-700"
              >
                {tag.name}
              </button>
            ))}
        </div>
      )}
      {error && <p className="text-xs text-red-600">{error}</p>}
    </div>
  );
}

function PlacementRow({ noteId }: { noteId: string }) {
  const { data: detail } = useNoteDetail(noteId);
  const { data: projects } = useProjects(true);
  const moveNote = useMoveNote();
  const [error, setError] = useState<string | null>(null);

  const projectId = detail?.note.project_id ?? null;
  const { data: tree } = useFolderTree(projectId);

  function flatten(
    nodes: FolderNode[] | undefined,
    depth: number,
  ): { id: string; label: string }[] {
    if (!nodes) return [];
    return nodes.flatMap((node) => [
      { id: node.folder.id, label: `${"— ".repeat(depth)}${node.folder.name}` },
      ...flatten(node.children, depth + 1),
    ]);
  }

  if (!detail) return null;

  return (
    <div className="flex flex-wrap items-center gap-2 text-sm">
      <select
        aria-label="Project"
        value={projectId ?? ""}
        onChange={(event) => {
          const next = event.target.value || null;
          moveNote.mutate(
            { id: noteId, projectId: next, folderId: null },
            { onError: (err) => setError(errorMessage(err)) },
          );
        }}
        className="rounded-md border border-neutral-300 bg-white px-2 py-1 text-sm dark:border-neutral-700 dark:bg-neutral-900"
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
      {projectId && (
        <select
          aria-label="Folder"
          value={detail.note.folder_id ?? ""}
          onChange={(event) => {
            const next = event.target.value || null;
            moveNote.mutate(
              { id: noteId, projectId, folderId: next },
              { onError: (err) => setError(errorMessage(err)) },
            );
          }}
          className="rounded-md border border-neutral-300 bg-white px-2 py-1 text-sm dark:border-neutral-700 dark:bg-neutral-900"
        >
          <option value="">No folder</option>
          {flatten(tree, 0).map((folder) => (
            <option key={folder.id} value={folder.id}>
              {folder.label}
            </option>
          ))}
        </select>
      )}
      {error && <span className="text-xs text-red-600">{error}</span>}
    </div>
  );
}

/** Full-width note editor with source/preview toggle and autosave. */
export function NoteEditor({ noteId }: { noteId: string }) {
  const openNote = useUi((state) => state.openNote);
  const editorMode = useUi((state) => state.editorMode);
  const setEditorMode = useUi((state) => state.setEditorMode);

  const { data: detail, isPending, error } = useNoteDetail(noteId);
  const updateNote = useUpdateNote();
  const deleteNote = useDeleteNote();
  const branchNote = useBranchNote();

  const [title, setTitle] = useState("");
  const [content, setContent] = useState("");
  const [saved, setSaved] = useState({ title: "", content: "" });
  const [saveState, setSaveState] = useState<"saved" | "dirty" | "saving">(
    "saved",
  );
  const [actionError, setActionError] = useState<string | null>(null);
  const [prevDetail, setPrevDetail] = useState<NoteDetail | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Adopt server state when new detail arrives while the editor is pristine
  // (no unsaved edits). This render-phase adjustment keeps typing intact
  // across background refetches. `key={noteId}` remounts between notes.
  if (detail && detail !== prevDetail) {
    setPrevDetail(detail);
    if (title === saved.title && content === saved.content) {
      setTitle(detail.note.title);
      setContent(detail.note.content);
      setSaved({ title: detail.note.title, content: detail.note.content });
      setSaveState("saved");
    }
  }

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "e") {
        event.preventDefault();
        setEditorMode(editorMode === "source" ? "preview" : "source");
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [editorMode, setEditorMode]);

  function scheduleSave(nextTitle: string, nextContent: string) {
    setSaveState("dirty");
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => {
      const patch: { title?: string; content?: string } = {};
      if (nextTitle !== saved.title) patch.title = nextTitle;
      if (nextContent !== saved.content) patch.content = nextContent;
      if (Object.keys(patch).length === 0) {
        setSaveState("saved");
        return;
      }
      setSaveState("saving");
      updateNote.mutate(
        { id: noteId, ...patch },
        {
          onSuccess: () => {
            setSaved({ title: nextTitle, content: nextContent });
            setSaveState("saved");
          },
          onError: () => setSaveState("dirty"),
        },
      );
    }, 800);
  }

  function onTitleChange(next: string) {
    setTitle(next);
    scheduleSave(next, content);
  }

  function onContentChange(next: string) {
    setContent(next);
    scheduleSave(title, next);
  }

  if (isPending)
    return <p className="text-sm text-neutral-500">Loading note…</p>;
  if (error)
    return <p className="text-sm text-red-600">{errorMessage(error)}</p>;
  if (!detail)
    return <p className="text-sm text-neutral-500">Note not found.</p>;

  return (
    <div className="flex h-full flex-col gap-3 overflow-y-auto">
      <div className="flex items-center gap-2">
        <button
          type="button"
          title="Back to notes"
          onClick={() => openNote(null)}
          className="rounded-md border border-neutral-300 p-1.5 dark:border-neutral-700"
        >
          <ArrowLeft size={15} />
        </button>
        <input
          value={title}
          onChange={(event) => onTitleChange(event.target.value)}
          placeholder="Note title"
          className="w-full bg-transparent text-xl font-semibold outline-none"
        />
        <span className="flex shrink-0 items-center gap-1 text-xs text-neutral-400">
          <Save size={12} />
          {saveState === "saved"
            ? "Saved"
            : saveState === "saving"
              ? "Saving…"
              : "Unsaved changes"}
        </span>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <PlacementRow noteId={noteId} />
        <span className="flex-1" />
        <div className="flex rounded-md border border-neutral-300 dark:border-neutral-700">
          <button
            type="button"
            title="Source (Ctrl+E)"
            onClick={() => setEditorMode("source")}
            className={`flex items-center gap-1 rounded-l-md px-2 py-1 text-xs ${editorMode === "source" ? "bg-neutral-200 dark:bg-neutral-700" : ""}`}
          >
            <Pencil size={12} />
            Source
          </button>
          <button
            type="button"
            title="Preview (Ctrl+E)"
            onClick={() => setEditorMode("preview")}
            className={`flex items-center gap-1 rounded-r-md px-2 py-1 text-xs ${editorMode === "preview" ? "bg-neutral-200 dark:bg-neutral-700" : ""}`}
          >
            <Eye size={12} />
            Preview
          </button>
        </div>
        <button
          type="button"
          title="Branch note"
          onClick={() =>
            branchNote.mutate(
              { id: noteId },
              {
                onSuccess: (branched) => openNote(branched.id),
                onError: (err) => setActionError(errorMessage(err)),
              },
            )
          }
          className="rounded-md border border-neutral-300 p-1.5 dark:border-neutral-700"
        >
          <GitBranch size={14} />
        </button>
        <button
          type="button"
          title="Delete note"
          onClick={() => {
            if (
              window.confirm(
                "Delete this note? Links and tags are removed with it.",
              )
            ) {
              deleteNote.mutate(noteId, {
                onSuccess: () => openNote(null),
                onError: (err) => setActionError(errorMessage(err)),
              });
            }
          }}
          className="rounded-md border border-neutral-300 p-1.5 dark:border-neutral-700"
        >
          <Trash2 size={14} />
        </button>
      </div>
      {actionError && <p className="text-sm text-red-600">{actionError}</p>}

      {editorMode === "source" ? (
        <div className="rounded-lg border border-neutral-300 bg-white p-2 dark:border-neutral-700 dark:bg-neutral-900">
          <CodeMirrorEditor
            key={noteId}
            initialDoc={content}
            onChange={onContentChange}
          />
        </div>
      ) : (
        <div className="markdown-body rounded-lg border border-neutral-300 bg-white p-4 dark:border-neutral-700 dark:bg-neutral-900">
          <ReactMarkdown
            rehypePlugins={[rehypeSanitize]}
            components={{
              a: ({ href, children }) => {
                const target = wikilinkTitleFromHref(href);
                if (target) {
                  return (
                    <WikilinkAnchor title={target} openNote={openNote}>
                      {children}
                    </WikilinkAnchor>
                  );
                }
                return (
                  <a href={href} target="_blank" rel="noreferrer">
                    {children}
                  </a>
                );
              },
            }}
          >
            {renderWikilinksAsLinks(content)}
          </ReactMarkdown>
        </div>
      )}

      <TagRow noteId={noteId} />
      <LinksPanel noteId={noteId} content={content} links={detail.links} />
    </div>
  );
}

function WikilinkAnchor({
  title,
  openNote,
  children,
}: {
  title: string;
  openNote: (id: string | null) => void;
  children: ReactNode;
}) {
  const { data: matches } = useTitleMatches(title);
  const target = (matches ?? [])[0];
  return (
    <button
      type="button"
      title={target ? `Open "${target.title}"` : "No matching note"}
      onClick={() => {
        if (target) openNote(target.id);
      }}
      className={
        target
          ? "text-blue-700 underline dark:text-blue-400"
          : "text-neutral-400 underline decoration-dotted"
      }
    >
      {children}
    </button>
  );
}
