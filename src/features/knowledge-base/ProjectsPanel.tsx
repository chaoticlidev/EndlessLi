import { useState } from "react";
import {
  Archive,
  ChevronDown,
  ChevronRight,
  FolderPlus,
  Inbox,
  Pencil,
  Plus,
  Trash2,
} from "lucide-react";
import { useUi } from "../../stores/ui";
import {
  errorMessage,
  useArchiveProject,
  useCreateFolder,
  useCreateProject,
  useDeleteFolder,
  useDeleteProject,
  useFolderTree,
  useProjects,
  useRenameFolder,
} from "../../ipc/queries";
import type { FolderNode } from "../../ipc/bindings";
import { TagManager } from "./TagManager";

function FolderRow({
  node,
  depth,
  projectId,
}: {
  node: FolderNode;
  depth: number;
  projectId: string;
}) {
  const selectedFolderId = useUi((state) => state.selectedFolderId);
  const selectFolder = useUi((state) => state.selectFolder);
  const [expanded, setExpanded] = useState(true);
  const [renaming, setRenaming] = useState(false);
  const [addingChild, setAddingChild] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const renameFolder = useRenameFolder();
  const deleteFolder = useDeleteFolder();
  const createFolder = useCreateFolder();

  const active = selectedFolderId === node.folder.id;

  async function submitRename() {
    const name = draft.trim();
    if (!name) {
      setRenaming(false);
      return;
    }
    try {
      await renameFolder.mutateAsync({ id: node.folder.id, name });
      setRenaming(false);
      setError(null);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function submitChild() {
    const name = draft.trim();
    if (!name) {
      setAddingChild(false);
      return;
    }
    try {
      await createFolder.mutateAsync({
        name,
        projectId,
        parentId: node.folder.id,
      });
      setAddingChild(false);
      setDraft("");
      setError(null);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <div>
      <div
        className={`group flex items-center gap-1 rounded-md px-1.5 py-1 text-sm ${
          active
            ? "bg-neutral-200 dark:bg-neutral-700"
            : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
        }`}
        style={{ paddingLeft: `${depth * 12 + 6}px` }}
      >
        <button
          type="button"
          aria-label={expanded ? "Collapse folder" : "Expand folder"}
          onClick={() => setExpanded(!expanded)}
          className="rounded p-0.5 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-700"
        >
          {expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
        </button>
        <button
          type="button"
          onClick={() => selectFolder(active ? null : node.folder.id)}
          className="min-w-0 flex-1 truncate text-left"
        >
          {renaming ? (
            <input
              autoFocus
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onBlur={() => void submitRename()}
              onKeyDown={(event) => {
                if (event.key === "Enter") void submitRename();
                if (event.key === "Escape") setRenaming(false);
              }}
              onClick={(event) => event.stopPropagation()}
              className="w-full rounded border border-neutral-300 bg-white px-1 text-sm dark:border-neutral-600 dark:bg-neutral-900"
            />
          ) : (
            node.folder.name
          )}
        </button>
        <span className="hidden items-center gap-0.5 group-hover:flex">
          <button
            type="button"
            title="New subfolder"
            onClick={() => {
              setAddingChild(true);
              setDraft("");
            }}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-700"
          >
            <FolderPlus size={13} />
          </button>
          <button
            type="button"
            title="Rename folder"
            onClick={() => {
              setRenaming(true);
              setDraft(node.folder.name);
            }}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-700"
          >
            <Pencil size={13} />
          </button>
          <button
            type="button"
            title="Delete folder"
            onClick={() => {
              if (
                window.confirm(
                  `Delete folder "${node.folder.name}"? Notes inside keep their content.`,
                )
              ) {
                deleteFolder.mutate(node.folder.id, {
                  onError: (err) => setError(errorMessage(err)),
                });
              }
            }}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-700"
          >
            <Trash2 size={13} />
          </button>
        </span>
      </div>
      {error && <p className="px-2 text-xs text-red-600">{error}</p>}
      {addingChild && (
        <div
          className="flex gap-1 px-2 py-1"
          style={{ paddingLeft: `${depth * 12 + 18}px` }}
        >
          <input
            autoFocus
            value={draft}
            placeholder="Folder name"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void submitChild();
              if (event.key === "Escape") setAddingChild(false);
            }}
            className="w-full rounded border border-neutral-300 bg-white px-1.5 py-0.5 text-sm dark:border-neutral-600 dark:bg-neutral-900"
          />
        </div>
      )}
      {expanded &&
        node.children.map((child) => (
          <FolderRow
            key={child.folder.id}
            node={child}
            depth={depth + 1}
            projectId={projectId}
          />
        ))}
    </div>
  );
}

function ProjectRow({ id, name }: { id: string; name: string }) {
  const selectedProjectId = useUi((state) => state.selectedProjectId);
  const selectProject = useUi((state) => state.selectProject);
  const selectFolder = useUi((state) => state.selectFolder);
  const [addingFolder, setAddingFolder] = useState(false);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);

  const { data: tree } = useFolderTree(selectedProjectId === id ? id : null);
  const createFolder = useCreateFolder();
  const archiveProject = useArchiveProject();
  const deleteProject = useDeleteProject();

  const active = selectedProjectId === id;

  async function submitFolder() {
    const name = draft.trim();
    if (!name) {
      setAddingFolder(false);
      return;
    }
    try {
      await createFolder.mutateAsync({ name, projectId: id, parentId: null });
      setAddingFolder(false);
      setDraft("");
      setError(null);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <div className="rounded-md">
      <div
        className={`group flex items-center gap-1 rounded-md px-1.5 py-1 text-sm font-medium ${
          active
            ? "bg-neutral-200 dark:bg-neutral-700"
            : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
        }`}
      >
        <button
          type="button"
          onClick={() => {
            selectProject(active ? null : id);
            selectFolder(null);
          }}
          className="min-w-0 flex-1 truncate text-left"
        >
          {name}
        </button>
        <span className="hidden items-center gap-0.5 group-hover:flex">
          <button
            type="button"
            title="New folder"
            onClick={() => setAddingFolder(true)}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-700"
          >
            <FolderPlus size={13} />
          </button>
          <button
            type="button"
            title="Archive project"
            onClick={() =>
              archiveProject.mutate(
                { id, archived: true },
                { onError: (err) => setError(errorMessage(err)) },
              )
            }
            className="rounded p-1 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-700"
          >
            <Archive size={13} />
          </button>
          <button
            type="button"
            title="Delete project"
            onClick={() => {
              if (
                window.confirm(
                  `Delete project "${name}"? Its folders are removed; notes keep their content.`,
                )
              ) {
                deleteProject.mutate(id, {
                  onError: (err) => setError(errorMessage(err)),
                });
              }
            }}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-700"
          >
            <Trash2 size={13} />
          </button>
        </span>
      </div>
      {error && <p className="px-2 text-xs text-red-600">{error}</p>}
      {addingFolder && (
        <div className="flex gap-1 px-2 py-1">
          <input
            autoFocus
            value={draft}
            placeholder="Folder name"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void submitFolder();
              if (event.key === "Escape") setAddingFolder(false);
            }}
            className="w-full rounded border border-neutral-300 bg-white px-1.5 py-0.5 text-sm dark:border-neutral-600 dark:bg-neutral-900"
          />
        </div>
      )}
      {active && tree && tree.length > 0 && (
        <div className="mt-0.5">
          {tree.map((node) => (
            <FolderRow
              key={node.folder.id}
              node={node}
              depth={1}
              projectId={id}
            />
          ))}
        </div>
      )}
    </div>
  );
}

/** Left panel: Inbox, projects with nested folders, and creation controls. */
export function ProjectsPanel() {
  const selectedProjectId = useUi((state) => state.selectedProjectId);
  const selectProject = useUi((state) => state.selectProject);
  const selectFolder = useUi((state) => state.selectFolder);

  const { data: projects, isPending, error } = useProjects(true);
  const createProject = useCreateProject();
  const archiveProject = useArchiveProject();
  const [draft, setDraft] = useState("");
  const [creating, setCreating] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const activeProjects = (projects ?? []).filter(
    (project) => !project.archived,
  );
  const archivedProjects = (projects ?? []).filter(
    (project) => project.archived,
  );

  async function submitProject() {
    const name = draft.trim();
    if (!name) {
      setCreating(false);
      return;
    }
    try {
      await createProject.mutateAsync(name);
      setDraft("");
      setCreating(false);
      setFormError(null);
    } catch (err) {
      setFormError(errorMessage(err));
    }
  }

  return (
    <div className="flex h-full flex-col gap-2">
      <button
        type="button"
        onClick={() => {
          selectProject(null);
          selectFolder(null);
        }}
        className={`flex items-center gap-2 rounded-md border px-2.5 py-1.5 text-sm ${
          selectedProjectId === null
            ? "border-neutral-900 bg-neutral-900 text-white dark:border-white dark:bg-white dark:text-neutral-900"
            : "border-neutral-300 dark:border-neutral-700"
        }`}
      >
        <Inbox size={15} />
        Inbox
      </button>

      <div className="flex items-center justify-between">
        <h2 className="px-1 text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Projects
        </h2>
        <button
          type="button"
          title="New project"
          onClick={() => setCreating(true)}
          className="rounded p-1 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-700"
        >
          <Plus size={14} />
        </button>
      </div>

      {creating && (
        <input
          autoFocus
          value={draft}
          placeholder="Project name"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void submitProject();
            if (event.key === "Escape") setCreating(false);
          }}
          onBlur={() => {
            if (!draft.trim()) setCreating(false);
          }}
          className="rounded border border-neutral-300 bg-white px-2 py-1 text-sm dark:border-neutral-600 dark:bg-neutral-900"
        />
      )}
      {formError && <p className="text-xs text-red-600">{formError}</p>}

      {isPending && (
        <p className="px-1 text-sm text-neutral-500">Loading projects…</p>
      )}
      {error && (
        <p className="px-1 text-sm text-red-600">{errorMessage(error)}</p>
      )}

      <div className="flex flex-col gap-0.5 overflow-y-auto">
        {activeProjects.map((project) => (
          <ProjectRow key={project.id} id={project.id} name={project.name} />
        ))}
      </div>

      {archivedProjects.length > 0 && (
        <div className="mt-1 border-t border-neutral-200 pt-1 dark:border-neutral-700">
          <h3 className="px-1 text-xs font-semibold uppercase tracking-wide text-neutral-400">
            Archived
          </h3>
          {archivedProjects.map((project) => (
            <div
              key={project.id}
              className="group flex items-center gap-1 px-1.5 py-1 text-sm text-neutral-500"
            >
              <span className="min-w-0 flex-1 truncate">{project.name}</span>
              <button
                type="button"
                title="Unarchive project"
                onClick={() =>
                  archiveProject.mutate({ id: project.id, archived: false })
                }
                className="hidden rounded p-1 hover:bg-neutral-200 group-hover:block dark:hover:bg-neutral-700"
              >
                <Archive size={13} />
              </button>
            </div>
          ))}
        </div>
      )}

      <TagManager />
    </div>
  );
}
