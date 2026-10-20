import { useState } from "react";
import {
  Copy,
  Eraser,
  Layers,
  Play,
  Plus,
  Save,
  Trash2,
} from "lucide-react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import type { ParamValue, Skill } from "../../ipc/bindings";
import {
  errorMessage,
  useDeleteSkill,
  useParsedSkill,
  useProjects,
  useRunSkill,
  useSaveSkill,
  useSaveSkillOutput,
  useSkills,
} from "../../ipc/queries";
import { CodeMirrorEditor } from "../../components/CodeMirrorEditor";
import { Select } from "../../components/Select";
import { EMPTY_VALUES, useWorkspace } from "../../stores/workspace";
import { ParamFields } from "./ParamFields";
import { NEW_SKILL_TEMPLATE } from "./skillsTypes";

const buttonBase =
  "flex items-center gap-1.5 rounded-md border border-neutral-300 px-3 py-1.5 text-sm dark:border-neutral-700";

/** Skills sub-tab: list on the left, editor and run panel on the right. */
export function SkillWorkspace() {
  const { data: skills, isPending, error } = useSkills();
  const selected = useWorkspace((state) => state.selectedSkill);
  const setSelected = useWorkspace((state) => state.setSelectedSkill);
  const [creating, setCreating] = useState(false);

  const items = skills ?? [];
  // Nothing chosen yet (or the chosen one is gone): fall back to the first.
  const effectiveSelected = creating
    ? null
    : selected && items.some((skill) => skill.name === selected)
      ? selected
      : (items[0]?.name ?? null);
  const current =
    items.find((skill) => skill.name === effectiveSelected) ?? null;

  return (
    <div className="flex h-full min-h-0 gap-3">
      <aside className="flex w-60 shrink-0 flex-col gap-1 overflow-y-auto rounded-lg border border-neutral-300 bg-white p-2 dark:border-neutral-700 dark:bg-neutral-900">
        <button
          type="button"
          onClick={() => {
            setCreating(true);
            setSelected(null);
          }}
          className={`${buttonBase} justify-center bg-neutral-900 text-white dark:bg-white dark:text-neutral-900`}
        >
          <Plus size={14} />
          New skill
        </button>
        {isPending && <p className="p-1 text-sm text-neutral-500">Loading…</p>}
        {items.map((skill) => (
          <button
            key={skill.name}
            type="button"
            onClick={() => {
              setCreating(false);
              setSelected(skill.name);
            }}
            className={`flex flex-col items-start gap-0.5 rounded-md px-2 py-1.5 text-left ${
              skill.name === effectiveSelected
                ? "bg-neutral-200 dark:bg-neutral-700"
                : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
            }`}
          >
            <span className="w-full truncate text-sm font-medium">
              {skill.name}
            </span>
            <span className="w-full truncate text-xs text-neutral-500">
              {skill.description || skill.category}
            </span>
          </button>
        ))}
        {error && <p className="text-xs text-red-600">{errorMessage(error)}</p>}
      </aside>

      <section className="flex min-w-0 flex-1 flex-col gap-2 overflow-y-auto">
        {creating ? (
          <SkillEditor
            key="__new__"
            skill={null}
            onSaved={(skill) => {
              setCreating(false);
              setSelected(skill.name);
            }}
            onDeleted={() => setSelected(null)}
          />
        ) : current ? (
          <SkillEditor
            key={current.name}
            skill={current}
            onSaved={(skill) => setSelected(skill.name)}
            onDeleted={() => setSelected(null)}
          />
        ) : (
          <p className="rounded-md border border-dashed border-neutral-300 p-6 text-center text-sm text-neutral-500 dark:border-neutral-700">
            Select a skill, or create one.
          </p>
        )}
      </section>
    </div>
  );
}

function SkillEditor({
  skill,
  onSaved,
  onDeleted,
}: {
  skill: Skill | null;
  onSaved: (skill: Skill) => void;
  onDeleted: () => void;
}) {
  const draftKey = skill?.name ?? "__new__";
  const [doc, setDoc] = useState(skill?.content ?? NEW_SKILL_TEMPLATE);
  const [noteTitle, setNoteTitle] = useState(skill?.name ?? "");
  const [noteProject, setNoteProject] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const save = useSaveSkill();
  const remove = useDeleteSkill();
  const run = useRunSkill();
  const saveOutput = useSaveSkillOutput();
  const parsed = useParsedSkill(skill);
  const { data: projects } = useProjects(true);

  const values = useWorkspace(
    (state) => state.skillValues[draftKey] ?? EMPTY_VALUES,
  );
  const outcome = useWorkspace((state) =>
    skill ? state.skillOutcomes[skill.name] : undefined,
  );
  const savedNote = useWorkspace((state) =>
    skill ? state.savedNotes[`skill:${skill.name}`] : undefined,
  );
  const setValue = useWorkspace((state) => state.setSkillValue);
  const setOutcome = useWorkspace((state) => state.setSkillOutcome);
  const markSaved = useWorkspace((state) => state.markSavedNote);
  const clearOutput = useWorkspace((state) => state.clearSkillOutput);

  const parameters = parsed.data?.parameters ?? [];
  const effective = (name: string, fallback: string | null) =>
    (values[name] ?? fallback ?? "").trim();
  const canRun =
    skill !== null &&
    !run.isPending &&
    parameters.every(
      (param) => !param.required || effective(param.name, param.default).length > 0,
    );

  const projectOptions = [
    { value: "", label: "Inbox" },
    ...(projects ?? [])
      .filter((project) => !project.archived)
      .map((project) => ({ value: project.id, label: project.name })),
  ];
  const noteLocation = noteProject
    ? ((projects ?? []).find((project) => project.id === noteProject)?.name ??
      "project")
    : "Inbox";
  const noteTitleValue = noteTitle.trim() || skill?.name || "Note";
  // One save per distinct output: double-clicking (or returning to the tab)
  // must not create duplicate notes.
  const alreadySaved =
    outcome !== undefined && savedNote?.output === outcome.output;

  async function handleSave() {
    setError(null);
    setNotice(null);
    try {
      const saved = await save.mutateAsync({
        document: doc,
        previousName: skill?.name ?? null,
      });
      setNotice(`Saved "${saved.name}".`);
      onSaved(saved);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function handleRun() {
    if (!skill) return;
    setError(null);
    setNotice(null);
    const payload: ParamValue[] = parameters.map((param) => ({
      name: param.name,
      value: effective(param.name, param.default),
    }));
    try {
      const result = await run.mutateAsync({
        name: skill.name,
        values: payload,
      });
      setOutcome(skill.name, result);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function handleDelete() {
    if (!skill) return;
    setError(null);
    setNotice(null);
    try {
      await remove.mutateAsync(skill.name);
      onDeleted();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function handleSaveNote() {
    if (!skill || !outcome) return;
    setError(null);
    setNotice(null);
    try {
      const note = await saveOutput.mutateAsync({
        name: outcome.skill,
        output: outcome.output,
        title: noteTitleValue,
        projectId: noteProject,
      });
      markSaved(`skill:${skill.name}`, {
        title: note.title,
        location: noteLocation,
        output: outcome.output,
      });
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  function handleCopy() {
    if (!outcome) return;
    void navigator.clipboard.writeText(outcome.output).then(
      () => setNotice("Output copied."),
      () => setError("Could not copy to the clipboard."),
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="flex items-center gap-1.5 text-sm font-semibold">
          <Layers size={15} />
          {skill ? skill.name : "New skill"}
        </span>
        <div className="ml-auto flex items-center gap-2">
          {skill && (
            <button
              type="button"
              onClick={() => void handleDelete()}
              disabled={remove.isPending}
              className={`${buttonBase} text-red-600 disabled:opacity-40 dark:text-red-400`}
            >
              <Trash2 size={14} />
              Delete
            </button>
          )}
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={save.isPending}
            className={`${buttonBase} bg-neutral-900 text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900`}
          >
            <Save size={14} />
            {save.isPending ? "Saving…" : "Save"}
          </button>
        </div>
      </div>

      {(notice || error) && (
        <p className={`text-sm ${error ? "text-red-600" : "text-neutral-500"}`}>
          {error ?? notice}
        </p>
      )}

      <div className="overflow-hidden rounded-lg border border-neutral-300 dark:border-neutral-700">
        <CodeMirrorEditor initialDoc={doc} onChange={setDoc} language="markdown" />
      </div>

      {parsed.error && (
        <p className="text-sm text-red-600">{errorMessage(parsed.error)}</p>
      )}

      <div className="flex flex-col gap-2 rounded-lg border border-neutral-300 bg-white p-3 dark:border-neutral-700 dark:bg-neutral-900">
        <span className="text-sm font-semibold">Run</span>
        {skill === null ? (
          <p className="text-xs text-neutral-500">
            Save the skill before running it.
          </p>
        ) : (
          <>
            <ParamFields
              parameters={parameters}
              values={values}
              onChange={(name, value) => setValue(draftKey, name, value)}
              disabled={run.isPending}
            />
            <div>
              <button
                type="button"
                onClick={() => void handleRun()}
                disabled={!canRun}
                className={`${buttonBase} bg-neutral-900 text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900`}
              >
                <Play size={14} />
                {run.isPending ? "Running…" : "Run skill"}
              </button>
            </div>
          </>
        )}

        {outcome && (
          <div className="flex flex-col gap-2 border-t border-neutral-200 pt-2 dark:border-neutral-700">
            <div className="flex items-center gap-3 text-xs text-neutral-500">
              <span>
                {outcome.model} · {outcome.usage.tokens_in} in /{" "}
                {outcome.usage.tokens_out} out
                {outcome.usage.estimated ? " (estimated)" : ""}
              </span>
              <button
                type="button"
                onClick={handleCopy}
                className="flex items-center gap-1 text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200"
              >
                <Copy size={12} />
                Copy
              </button>
              <button
                type="button"
                onClick={() => skill && clearOutput(skill.name)}
                className="flex items-center gap-1 text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200"
              >
                <Eraser size={12} />
                Clear
              </button>
            </div>
            {outcome.budget_warning && (
              <p className="text-xs text-amber-600">{outcome.budget_warning}</p>
            )}
            <div className="markdown-body max-h-80 overflow-y-auto rounded-md border border-neutral-200 p-3 dark:border-neutral-700">
              <ReactMarkdown rehypePlugins={[rehypeSanitize]}>
                {outcome.output}
              </ReactMarkdown>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <input
                value={noteTitle}
                onChange={(event) => setNoteTitle(event.target.value)}
                placeholder="Note title"
                className="min-w-40 flex-1 rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
              />
              <Select
                ariaLabel="Project for the saved note"
                value={noteProject ?? ""}
                onChange={(value) => setNoteProject(value || null)}
                options={projectOptions}
                className="min-w-40"
              />
              <button
                type="button"
                onClick={() => void handleSaveNote()}
                disabled={saveOutput.isPending || alreadySaved}
                className={`${buttonBase} disabled:opacity-40`}
              >
                <Save size={14} />
                {saveOutput.isPending
                  ? "Saving…"
                  : alreadySaved
                    ? "Saved"
                    : "Save as note"}
              </button>
            </div>
            {alreadySaved && savedNote && (
              <p className="text-xs text-green-700 dark:text-green-400">
                Saved to {savedNote.location} as &ldquo;{savedNote.title}&rdquo;.
              </p>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
