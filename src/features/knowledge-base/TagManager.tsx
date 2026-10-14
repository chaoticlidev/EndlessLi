import { useState } from "react";
import { Tags, Trash2 } from "lucide-react";
import { errorMessage, useDeleteTag, useTags } from "../../ipc/queries";

/** Global tag list with delete. Per-note attach/detach lives in the editor. */
export function TagManager() {
  const { data: tags } = useTags();
  const deleteTag = useDeleteTag();
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState(false);

  if (!tags || tags.length === 0) return null;

  return (
    <div className="mt-1 border-t border-neutral-200 pt-1 dark:border-neutral-700">
      <button
        type="button"
        onClick={() => setExpanded(!expanded)}
        className="flex w-full items-center gap-1.5 px-1 py-1 text-xs font-semibold uppercase tracking-wide text-neutral-500"
      >
        <Tags size={12} />
        Tags ({tags.length})
      </button>
      {expanded && (
        <div className="flex flex-col gap-0.5">
          {tags.map((tag) => (
            <div
              key={tag.id}
              className="group flex items-center gap-1 rounded px-1.5 py-0.5 text-sm"
            >
              <span className="min-w-0 flex-1 truncate">{tag.name}</span>
              <button
                type="button"
                title={`Delete tag "${tag.name}"`}
                onClick={() => {
                  if (
                    window.confirm(`Delete tag "${tag.name}" from all notes?`)
                  ) {
                    deleteTag.mutate(tag.id, {
                      onError: (err) => setError(errorMessage(err)),
                    });
                  }
                }}
                className="hidden rounded p-1 text-neutral-400 hover:bg-neutral-200 group-hover:block dark:hover:bg-neutral-700"
              >
                <Trash2 size={12} />
              </button>
            </div>
          ))}
          {error && <p className="px-1 text-xs text-red-600">{error}</p>}
        </div>
      )}
    </div>
  );
}
