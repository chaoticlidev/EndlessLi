import { useState } from "react";
import { BookmarkPlus, Copy, Eraser, Play, Plus, Save, Terminal, Trash2 } from "lucide-react";
import type { ParamValue, Script } from "../../ipc/bindings";
import {
  errorMessage,
  useDeleteScript,
  useParsedScript,
  useProjects,
  useRunScript,
  useSaveScript,
  useSaveScriptOutput,
  useScriptRuntime,
  useScripts,
} from "../../ipc/queries";
import { CodeMirrorEditor } from "../../components/CodeMirrorEditor";
import { Markdown } from "../../components/Markdown";
import { Select } from "../../components/Select";
import { EMPTY_VALUES, useWorkspace } from "../../stores/workspace";
import { ParamFields } from "./ParamFields";
import { DEFAULT_SCRIPT_TIMEOUT_SECS, NEW_SCRIPT_TEMPLATE } from "./skillsTypes";

const buttonBase =
  "flex items-center gap-1.5 rounded-md border border-neutral-300 px-3 py-1.5 text-sm dark:border-neutral-700";

/** Set once the user has acknowledged what the network toggle does. */
const NETWORK_NOTICE_KEY = "endlessli.scriptNetworkNoticeAcknowledged";

/** Scripts sub-tab: list on the left, editor and run panel on the right. */
export function ScriptWorkspace() {
  const { data: scripts, isPending, error } = useScripts();
  const runtime = useScriptRuntime();
  const selected = useWorkspace((state) => state.selectedScript);
  const setSelected = useWorkspace((state) => state.setSelectedScript);
  const [creating, setCreating] = useState(false);

  const items = scripts ?? [];
  const effectiveSelected = creating
    ? null
    : selected && items.some((script) => script.name === selected)
      ? selected
      : (items[0]?.name ?? null);
  const current =
    items.find((script) => script.name === effectiveSelected) ?? null;

  const unavailable = runtime.data && !runtime.data.available;

  return (
    <div className="flex h-full min-h-0 gap-3">
      <aside className="flex w-60 shrink-0 flex-col gap-1 overflow-y-auto rounded-lg border border-border bg-panel/85 p-2 backdrop-blur-sm">
        <button
          type="button"
          onClick={() => {
            setCreating(true);
            setSelected(null);
          }}
          className={`${buttonBase} justify-center bg-neutral-900 text-white dark:bg-white dark:text-neutral-900`}
        >
          <Plus size={14} />
          New script
        </button>
        {isPending && <p className="p-1 text-sm text-neutral-500">Loading…</p>}
        {items.map((script) => (
          <button
            key={script.name}
            type="button"
            onClick={() => {
              setCreating(false);
              setSelected(script.name);
            }}
            className={`flex flex-col items-start gap-0.5 rounded-md px-2 py-1.5 text-left ${
              script.name === effectiveSelected
                ? "bg-neutral-200 dark:bg-neutral-700"
                : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
            }`}
          >
            <span className="w-full truncate text-sm font-medium">
              {script.name}
            </span>
            <span className="w-full truncate text-xs text-neutral-500">
              {script.allow_network ? "network allowed" : script.description}
            </span>
          </button>
        ))}
        {error && <p className="text-xs text-red-600">{errorMessage(error)}</p>}
      </aside>

      <section className="flex min-w-0 flex-1 flex-col gap-2 overflow-y-auto">
        {unavailable && (
          <p className="rounded-md border border-amber-400 bg-amber-50 p-2 text-sm text-amber-700 dark:bg-amber-950/40 dark:text-amber-300">
            No Python interpreter was found, so scripts cannot run. Install the
            dev venv (see Settings) or put Python on your PATH.
            {runtime.data?.interpreter
              ? ` Looked for: ${runtime.data.interpreter}.`
              : ""}
          </p>
        )}
        {creating ? (
          <ScriptEditor
            key="__new__"
            script={null}
            onSaved={(script) => {
              setCreating(false);
              setSelected(script.name);
            }}
            onDeleted={() => setSelected(null)}
          />
        ) : current ? (
          <ScriptEditor
            key={current.name}
            script={current}
            onSaved={(script) => setSelected(script.name)}
            onDeleted={() => setSelected(null)}
          />
        ) : (
          <p className="rounded-md border border-dashed border-border p-6 text-center text-sm text-muted-foreground">
            Select a script, or create one.
          </p>
        )}
      </section>
    </div>
  );
}

function ScriptEditor({
  script,
  onSaved,
  onDeleted,
}: {
  script: Script | null;
  onSaved: (script: Script) => void;
  onDeleted: () => void;
}) {
  const draftKey = script?.name ?? "__new__";
  const [doc, setDoc] = useState(script?.content ?? NEW_SCRIPT_TEMPLATE);
  const [timeoutSecs, setTimeoutSecs] = useState(DEFAULT_SCRIPT_TIMEOUT_SECS);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmNetwork, setConfirmNetwork] = useState(false);

  // Save-as-note state
  const [noteTitle, setNoteTitle] = useState(script?.name ?? "");
  const [noteProject, setNoteProject] = useState<string | null>(null);

  const save = useSaveScript();
  const remove = useDeleteScript();
  const saveOutput = useSaveScriptOutput();
  const run = useRunScript();
  const parsed = useParsedScript(script);
  const { data: projects } = useProjects(true);

  const values = useWorkspace(
    (state) => state.scriptValues[draftKey] ?? EMPTY_VALUES,
  );
  const allowOverride = useWorkspace((state) =>
    script ? state.scriptAllowNetwork[script.name] : undefined,
  );
  const outcome = useWorkspace((state) =>
    script ? state.scriptOutcomes[script.name] : undefined,
  );
  const savedNote = useWorkspace((state) =>
    script ? state.savedNotes[`script:${script.name}`] : undefined,
  );
  
  const setValue = useWorkspace((state) => state.setScriptValue);
  const setAllowNetwork = useWorkspace(
    (state) => state.setScriptAllowNetwork,
  );
  const setOutcome = useWorkspace((state) => state.setScriptOutcome);
  const markSaved = useWorkspace((state) => state.markSavedNote);
  const clearOutput = useWorkspace((state) => state.clearScriptOutput);
  const clearValues = useWorkspace((state) => state.clearScriptValues);

  const parameters = parsed.data?.parameters ?? [];
  const allowNetwork =
    allowOverride ?? parsed.data?.allow_network ?? false;

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
  const noteTitleValue = noteTitle.trim() || script?.name || "Note";
  const alreadySaved =
    outcome !== undefined && savedNote?.output === outcome.stdout;

  function handleNetworkToggle(enabled: boolean) {
    if (!script) return;
    if (enabled && localStorage.getItem(NETWORK_NOTICE_KEY) !== "1") {
      setConfirmNetwork(true);
      return;
    }
    setAllowNetwork(script.name, enabled);
  }

  function acceptNetwork() {
    if (!script) return;
    localStorage.setItem(NETWORK_NOTICE_KEY, "1");
    setAllowNetwork(script.name, true);
    setConfirmNetwork(false);
  }
  const effective = (name: string, fallback: string | null) =>
    (values[name] ?? fallback ?? "").trim();
  const canRun =
    script !== null &&
    !run.isPending &&
    parameters.every(
      (param) => !param.required || effective(param.name, param.default).length > 0,
    );

  async function handleSave() {
    setError(null);
    setNotice(null);
    try {
      const saved = await save.mutateAsync({
        document: doc,
        previousName: script?.name ?? null,
      });
      setNotice(`Saved "${saved.name}".`);
      onSaved(saved);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function handleRun() {
    if (!script) return;
    setError(null);
    setNotice(null);
    const payload: ParamValue[] = parameters.map((param) => ({
      name: param.name,
      value: effective(param.name, param.default),
    }));
    try {
      const result = await run.mutateAsync({
        name: script.name,
        values: payload,
        allowNetwork,
        timeoutSecs,
      });
      setOutcome(script.name, result);
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function handleDelete() {
    if (!script) return;
    setError(null);
    setNotice(null);
    try {
      await remove.mutateAsync(script.name);
      onDeleted();
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  async function handleSaveNote() {
    if (!script || !outcome) return;
    setError(null);
    setNotice(null);
    try {
      const note = await saveOutput.mutateAsync({
        name: script.name,
        output: outcome.stdout || outcome.stderr || "",
        title: noteTitleValue,
        projectId: noteProject,
      });
      markSaved(`script:${script.name}`, {
        title: note.title,
        location: noteLocation,
        output: outcome.stdout || outcome.stderr || "",
      });
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  function handleCopy() {
    if (!outcome) return;
    const textToCopy = outcome.stdout || outcome.stderr || "";
    void navigator.clipboard.writeText(textToCopy).then(
      () => setNotice("Output copied."),
      () => setError("Could not copy to the clipboard."),
    );
  }
  
  

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="flex items-center gap-1.5 text-sm font-semibold">
          <Terminal size={15} />
          {script ? script.name : "New script"}
        </span>
        {parsed.data?.allow_network && (
          <span className="rounded border border-amber-400 px-1.5 text-[10px] uppercase text-amber-600">
            requests network
          </span>
        )}
        <div className="ml-auto flex items-center gap-2">
          {script && (
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
        <CodeMirrorEditor initialDoc={doc} onChange={setDoc} language="python" />
      </div>

      {parsed.error && (
        <p className="text-sm text-red-600">{errorMessage(parsed.error)}</p>
      )}

      <div className="flex flex-col gap-2 rounded-lg border border-border bg-card/80 p-3 backdrop-blur-sm">
        <span className="text-sm font-semibold">Run</span>
        {script === null ? (
          <p className="text-xs text-neutral-500">
            Save the script before running it.
          </p>
        ) : (
          <>
            <ParamFields
              parameters={parameters}
              values={values}
              onChange={(name, value) => setValue(draftKey, name, value)}
              disabled={run.isPending}
            />
            <div className="flex flex-wrap items-center gap-3 text-sm">
              <label className="flex items-center gap-1.5">
                <input
                  type="checkbox"
                  checked={allowNetwork}
                  onChange={(event) => handleNetworkToggle(event.target.checked)}
                />
                Allow network
              </label>
              <span className="text-xs text-neutral-500">
                Blocks ordinary socket use; a determined script can bypass it.
              </span>
              <label className="flex items-center gap-1.5">
                Timeout
                <input
                  type="number"
                  min={1}
                  max={600}
                  value={timeoutSecs}
                  onChange={(event) =>
                    setTimeoutSecs(Number(event.target.value) || 1)
                  }
                  className="w-20 rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
                />
                s
              </label>
              <button
                type="button"
                onClick={() => void handleRun()}
                disabled={!canRun}
                className={`${buttonBase} bg-neutral-900 text-white disabled:opacity-40 dark:bg-white dark:text-neutral-900`}
              >
                <Play size={14} />
                {run.isPending ? "Running…" : "Run script"}
              </button>
            </div>
            {confirmNetwork && (
              <div className="rounded-md border border-amber-400 bg-amber-50 p-2 text-xs dark:bg-amber-950/30">
                <p>
                  Scripts run with your user permissions. This toggle blocks
                  ordinary socket access, but it is not an OS sandbox: a
                  determined script can still reach the network (for example
                  through subprocesses or native APIs). Only enable it for code
                  you trust.
                </p>
                <div className="mt-2 flex gap-2">
                  <button
                    type="button"
                    onClick={acceptNetwork}
                    className={`${buttonBase} bg-neutral-900 text-white dark:bg-white dark:text-neutral-900`}
                  >
                    Enable network
                  </button>
                  <button
                    type="button"
                    onClick={() => setConfirmNetwork(false)}
                    className={buttonBase}
                  >
                    Cancel
                  </button>
                </div>
              </div>
            )}
          </>
        )}

        {outcome && (
          <div className="flex flex-col gap-2 border-t border-neutral-200 pt-2 dark:border-neutral-700">
            <div className="flex flex-wrap items-center gap-3 text-xs text-neutral-500">
              {outcome.timed_out ? (
                <span className="rounded border border-amber-400 px-1.5 text-amber-600">
                  Timed out
                </span>
              ) : (
                <span>exit {outcome.exit_code ?? "?"}</span>
              )}
              <span>{outcome.duration_ms} ms</span>
              
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
                onClick={() => {
                  if (script) {
                    clearOutput(script.name);
                    clearValues(script.name);
                  }
                }}
                className="flex items-center gap-1 text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200"
              >
                <Eraser size={12} />
                Clear
              </button>
            </div>
            {outcome.stdout && (
              <div className="markdown-body max-h-80 overflow-y-auto rounded-md border border-neutral-200 p-3 dark:border-neutral-700">
                <Markdown content={outcome.stdout} />
              </div>
            )}
            {outcome.stderr && (
              <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-md border border-red-300 bg-red-50 p-3 text-xs text-red-700 dark:border-red-800 dark:bg-red-950/40 dark:text-red-300">
                {outcome.stderr}
              </pre>
            )}
            {!outcome.stdout && !outcome.stderr && (
              <p className="text-xs text-neutral-500">
                The script produced no output.
              </p>
            )}
            
            {/* Save as Note UI */}
            <div className="flex flex-wrap items-center gap-2 border-t border-neutral-200 pt-2 dark:border-neutral-700">
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
                <BookmarkPlus size={14} />
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