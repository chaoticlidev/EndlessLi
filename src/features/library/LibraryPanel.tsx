import { Globe, Tags } from "lucide-react";
import { useLibrarySources, useLibraryStats, useTags } from "../../ipc/queries";
import { LIBRARY_TYPES, typeLabel } from "./libraryTypes";

interface LibraryPanelProps {
  itemType: string | null;
  onItemType: (value: string | null) => void;
  tagId: string | null;
  onTagId: (value: string | null) => void;
  source: string | null;
  onSource: (value: string | null) => void;
}

function FilterButton({
  label,
  count,
  active,
  onClick,
}: {
  label: string;
  count?: number;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex w-full items-center justify-between gap-2 rounded-md px-2 py-1 text-left text-sm ${
        active
          ? "bg-neutral-200 dark:bg-neutral-700"
          : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
      }`}
    >
      <span className="min-w-0 flex-1 truncate">{label}</span>
      {count !== undefined && (
        <span className="shrink-0 text-xs text-neutral-400">{count}</span>
      )}
    </button>
  );
}

/** Library categories: All, Type, Tag, and Source filters. */
export function LibraryPanel({
  itemType,
  onItemType,
  tagId,
  onTagId,
  source,
  onSource,
}: LibraryPanelProps) {
  const { data: stats } = useLibraryStats();
  const { data: tags } = useTags();
  const { data: sources } = useLibrarySources();

  const counts = new Map(
    (stats?.by_type ?? []).map((entry) => [entry.item_type, entry.count]),
  );
  const visibleTypes = LIBRARY_TYPES.filter(
    (type) => (counts.get(type) ?? 0) > 0 || type === itemType,
  );

  return (
    <div className="flex flex-col gap-3">
      <FilterButton
        label="All items"
        count={stats?.total ?? 0}
        active={itemType === null && tagId === null && source === null}
        onClick={() => {
          onItemType(null);
          onTagId(null);
          onSource(null);
        }}
      />

      <div className="flex flex-col gap-0.5">
        <h2 className="px-1 text-xs font-semibold uppercase tracking-wide text-neutral-500">
          Types
        </h2>
        {visibleTypes.length === 0 && (
          <p className="px-1 text-xs text-neutral-500">Nothing imported yet.</p>
        )}
        {visibleTypes.map((type) => (
          <FilterButton
            key={type}
            label={typeLabel(type)}
            count={counts.get(type) ?? 0}
            active={itemType === type}
            onClick={() => onItemType(itemType === type ? null : type)}
          />
        ))}
      </div>

      {(tags ?? []).length > 0 && (
        <div className="flex flex-col gap-0.5">
          <h2 className="flex items-center gap-1 px-1 text-xs font-semibold uppercase tracking-wide text-neutral-500">
            <Tags size={11} />
            Tags
          </h2>
          {(tags ?? []).map((tag) => (
            <FilterButton
              key={tag.id}
              label={tag.name}
              active={tagId === tag.id}
              onClick={() => onTagId(tagId === tag.id ? null : tag.id)}
            />
          ))}
        </div>
      )}

      {(sources ?? []).length > 0 && (
        <div className="flex flex-col gap-0.5">
          <h2 className="flex items-center gap-1 px-1 text-xs font-semibold uppercase tracking-wide text-neutral-500">
            <Globe size={11} />
            Sources
          </h2>
          {(sources ?? []).map((name) => (
            <FilterButton
              key={name}
              label={name}
              active={source === name}
              onClick={() => onSource(source === name ? null : name)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
