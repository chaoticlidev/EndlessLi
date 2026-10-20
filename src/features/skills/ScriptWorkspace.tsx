import { useState } from "react";
import { Eraser, Play, Plus, Save, Terminal, Trash2 } from "lucide-react";
import type { ParamValue, Script } from "../../ipc/bindings";
import {
  errorMessage,
  useDeleteScript,
  useParsedScript,
  useRunScript,
  useSaveScript,
  useScriptRuntime,
  useScripts,
} from "../../ipc/queries";
import { CodeMirrorEditor } from "../../components/CodeMirrorEditor";
import { EMPTY_VALUES, useWorkspace } from "../../stores/workspace";
import { ParamFields } from "./ParamFields";
import { DEFAULT_SCRIPT_TIMEOUT_SECS, NEW_SCRIPT_TEMPLATE } from "./skillsTypes";

const buttonBase =
  "flex items-center gap-1.5 rounded-md border border-neutral-300 px-3 py-1.5 text-sm dark:border-neutral-700";

/** Scripts sub-tab: list on the left, editor and run panel on the right. */
export function ScriptWorkspace() {
  const { data: scripts, isPending, error } = useScripts();
  const runtime = useScriptRuntime();
  const selected = useWorkspace((state) => state.selectedScript);
  const setSelected = useWorkspace((state) => state.setSelectedScript);
  const [creating, setCreating] = useState(false);

  const items = scripts ?? [];
  // Nothing chosen yet (or the chosen one is gone): fall back to the first.
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
          <p className="rounded-md border border-dashed border-neutral-300 p-6 text-center text-sm text-neutral-500 dark:border-neutral-700">
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

  const save = useSaveScript();
  const remove = useDeleteScript();
  const run = useRunScript();
  const parsed = useParsedScript(script);

  const values = useWorkspace(
    (state) => state.scriptValues[draftKey] ?? EMPTY_VALUES,
  );
  const allowOverride = useWorkspace((state) =>
    script ? state.scriptAllowNetwork[script.name] : undefined,
  );
  const outcome = useWorkspace((state) =>
    script ? state.scriptOutcomes[script.name] : undefined,
  );
  const setValue = useWorkspace((state) => state.setScriptValue);
  const setAllowNetwork = useWorkspace(
    (state) => state.setScriptAllowNetwork,
  );
  const setOutcome = useWorkspace((state) => state.setScriptOutcome);
  const clearOutput = useWorkspace((state) => state.clearScriptOutput);

  const parameters = parsed.data?.parameters ?? [];
  const allowNetwork =
    allowOverride ?? parsed.data?.allow_network ?? false;
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

      <div className="flex flex-col gap-2 rounded-lg border border-neutral-300 bg-white p-3 dark:border-neutral-700 dark:bg-neutral-900">
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
                  onChange={(event) =>
                    script && setAllowNetwork(script.name, event.target.checked)
                  }
                />
                Allow network
              </label>
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
                onClick={() => script && clearOutput(script.name)}
                className="flex items-center gap-1 text-neutral-500 hover:text-neutral-800 dark:hover:text-neutral-200"
              >
                <Eraser size={12} />
                Clear
              </button>
            </div>
            {outcome.stdout && (
              <pre className="max-h-80 overflow-auto whitespace-pre-wrap rounded-md border border-neutral-200 bg-neutral-50 p-3 text-xs dark:border-neutral-700 dark:bg-neutral-950">
                {outcome.stdout}
              </pre>
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
          </div>
        )}
      </div>
    </div>
  );
}
