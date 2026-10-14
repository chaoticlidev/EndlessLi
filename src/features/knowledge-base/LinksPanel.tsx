import { useState } from "react";
import { Link2, Plus, Search, Unlink, X } from "lucide-react";
import { useUi } from "../../stores/ui";
import {
  errorMessage,
  useCreateLink,
  useNoteSearch,
  useRemoveLink,
  useTitleMatches,
} from "../../ipc/queries";
import type { LinkedNote } from "../../ipc/bindings";
import { parseWikilinks } from "./wikilinks";

/** Linked-notes panel: current links, `[[wikilink]]` resolution, add-by-search. */
export function LinksPanel({
  noteId,
  content,
  links,
}: {
  noteId: string;
  content: string;
  links: LinkedNote[];
}) {
  const openNote = useUi((state) => state.openNote);
  const [searchText, setSearchText] = useState("");
  const [error, setError] = useState<string | null>(null);

  const createLink = useCreateLink();
  const removeLink = useRemoveLink();
  const { data: searchResults } = useNoteSearch(searchText.trim());
  const linkedIds = new Set(links.map((link) => link.id));
  const wikilinks = parseWikilinks(content).filter((title) => title.length > 0);

  async function linkTo(targetId: string) {
    setError(null);
    try {
      await createLink.mutateAsync({ a: noteId, b: targetId });
      setSearchText("");
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <section className="flex flex-col gap-2 rounded-lg border border-neutral-300 p-3 dark:border-neutral-700">
      <h3 className="flex items-center gap-1.5 text-sm font-semibold">
        <Link2 size={14} />
        Linked notes ({links.length})
      </h3>

      {links.length === 0 && (
        <p className="text-xs text-neutral-500">No links yet.</p>
      )}
      <ul className="flex flex-col gap-1">
        {links.map((link) => (
          <li key={link.id} className="group flex items-center gap-1.5 text-sm">
            <button
              type="button"
              onClick={() => openNote(link.id)}
              className="min-w-0 flex-1 truncate rounded px-1 py-0.5 text-left hover:bg-neutral-100 dark:hover:bg-neutral-800"
              title={
                link.origin === "manual" ? "Manual link" : "Approved suggestion"
              }
            >
              {link.title}
            </button>
            <button
              type="button"
              title="Remove link"
              onClick={() => removeLink.mutate({ a: noteId, b: link.id })}
              className="hidden rounded p-1 text-neutral-400 hover:bg-neutral-100 group-hover:block dark:hover:bg-neutral-800"
            >
              <Unlink size={13} />
            </button>
          </li>
        ))}
      </ul>

      {wikilinks.length > 0 && (
        <div className="flex flex-col gap-1 border-t border-neutral-200 pt-2 dark:border-neutral-700">
          <h4 className="text-xs font-semibold uppercase tracking-wide text-neutral-500">
            Wikilinks in this note
          </h4>
          {wikilinks.map((title) => (
            <WikilinkRow
              key={title}
              noteId={noteId}
              title={title}
              onLink={(targetId) => void linkTo(targetId)}
            />
          ))}
        </div>
      )}

      <div className="flex items-center gap-1.5 rounded-md border border-neutral-300 px-2 py-1 dark:border-neutral-700">
        <Search size={13} className="shrink-0 text-neutral-400" />
        <input
          value={searchText}
          placeholder="Link to a note…"
          onChange={(event) => setSearchText(event.target.value)}
          className="w-full bg-transparent text-sm outline-none"
        />
        {searchText && (
          <button
            type="button"
            aria-label="Clear"
            onClick={() => setSearchText("")}
          >
            <X size={13} className="text-neutral-400" />
          </button>
        )}
      </div>
      {searchText.trim() && (
        <ul className="flex max-h-32 flex-col gap-0.5 overflow-y-auto">
          {(searchResults ?? [])
            .filter((note) => note.id !== noteId && !linkedIds.has(note.id))
            .slice(0, 6)
            .map((note) => (
              <li key={note.id}>
                <button
                  type="button"
                  onClick={() => void linkTo(note.id)}
                  className="flex w-full items-center gap-1.5 rounded px-1 py-1 text-left text-sm hover:bg-neutral-100 dark:hover:bg-neutral-800"
                >
                  <Plus size={13} className="shrink-0 text-neutral-400" />
                  <span className="truncate">{note.title}</span>
                </button>
              </li>
            ))}
        </ul>
      )}
      {error && <p className="text-xs text-red-600">{error}</p>}
    </section>
  );
}

function WikilinkRow({
  noteId,
  title,
  onLink,
}: {
  noteId: string;
  title: string;
  onLink: (targetId: string) => void;
}) {
  const openNote = useUi((state) => state.openNote);
  const { data: matches } = useTitleMatches(title);

  const exact = (matches ?? []).filter((note) => note.id !== noteId);

  return (
    <div className="flex items-center gap-1.5 text-sm">
      <span className="min-w-0 flex-1 truncate text-neutral-600 dark:text-neutral-300">
        [[{title}]]
      </span>
      {exact.length > 0 ? (
        <button
          type="button"
          title={`Link to "${exact[0].title}"`}
          onClick={() => {
            onLink(exact[0].id);
            openNote(exact[0].id);
          }}
          className="shrink-0 rounded border border-neutral-300 px-1.5 py-0.5 text-xs hover:bg-neutral-100 dark:border-neutral-600 dark:hover:bg-neutral-800"
        >
          Link
        </button>
      ) : (
        <span className="shrink-0 text-xs text-neutral-400">no match</span>
      )}
    </div>
  );
}
