import { useState } from "react";
import type { ReactNode } from "react";
import {
  Check,
  Database,
  KeyRound,
  Loader2,
  RefreshCw,
  Trash2,
  FlaskConical,
} from "lucide-react";
import { commands } from "../../ipc/bindings";
import type { MemoryEntry, Project, TestOutcome } from "../../ipc/bindings";
import {
  errorMessage,
  useAnalyticsStatus,
  useCacheStats,
  useClearCache,
  useDeleteKey,
  useEmbeddingStatus,
  useKeyStatus,
  useMemoryEnabled,
  useMemoryList,
  useForgottenMemories,
  useCreateMemory,
  useForgetMemory,
  useRestoreMemory,
  useDeleteMemory,
  useSetMemoryEnabled,
  useReindexEmbeddings,
  useSetEmbeddingModel,
  useSetKey,
  useProjects,
  useSetSetting,
  useSetting,
  useTokenSummary,
} from "../../ipc/queries";
import { Select } from "../../components/Select";

const LLM_PROVIDERS = [
  { id: "openai", label: "OpenAI", defaultModel: "gpt-4o-mini" },
  { id: "anthropic", label: "Anthropic", defaultModel: "claude-sonnet-4-5" },
  { id: "deepseek", label: "DeepSeek", defaultModel: "deepseek-chat" },
] as const;

const EMBEDDING_MODELS = [
  {
    id: "sentence-transformers/all-MiniLM-L6-v2",
    label: "MiniLM L6 v2 (384d, fast)",
  },
  { id: "BAAI/bge-small-en-v1.5", label: "BGE small en v1.5 (384d)" },
  { id: "BAAI/bge-base-en-v1.5", label: "BGE base en v1.5 (768d)" },
  {
    id: "snowflake/snowflake-arctic-embed-s",
    label: "Arctic embed s (384d)",
  },
  {
    id: "snowflake/snowflake-arctic-embed-m",
    label: "Arctic embed m (768d)",
  },
] as const;

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2 rounded-lg border border-neutral-300 bg-white p-4 dark:border-neutral-700 dark:bg-neutral-900">
      <h2 className="text-sm font-semibold">{title}</h2>
      {children}
    </section>
  );
}

function SettingField({
  settingKey,
  label,
  hint,
  placeholder,
}: {
  settingKey: string;
  label: string;
  hint?: string;
  placeholder?: string;
}) {
  const { data: stored } = useSetting(settingKey);
  const setSetting = useSetSetting();
  const [draft, setDraft] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const value = draft ?? stored ?? "";

  async function save() {
    setStatus(null);
    try {
      await setSetting.mutateAsync({
        key: settingKey,
        value: draft ?? stored ?? "",
      });
      setDraft(null);
      setStatus("Saved");
    } catch (err) {
      setStatus(errorMessage(err));
    }
  }

  return (
    <label className="flex flex-col gap-1 text-sm">
      <span className="font-medium">{label}</span>
      <span className="flex gap-1.5">
        <input
          value={value}
          placeholder={placeholder}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void save();
          }}
          className="w-full rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
        />
        <button
          type="button"
          onClick={() => void save()}
          disabled={draft === null}
          className="shrink-0 rounded-md border border-neutral-300 px-2.5 py-1 text-xs disabled:opacity-40 dark:border-neutral-600"
        >
          Save
        </button>
      </span>
      {hint && <span className="text-xs text-neutral-500">{hint}</span>}
      {status && (
        <span
          className={`text-xs ${status === "Saved" ? "text-green-700 dark:text-green-400" : "text-red-600"}`}
        >
          {status}
        </span>
      )}
    </label>
  );
}

function ProviderKeys() {
  const { data: status, error: statusError } = useKeyStatus();
  const setKey = useSetKey();
  const deleteKey = useDeleteKey();
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [message, setMessage] = useState<Record<string, string>>({});
  const [testing, setTesting] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<{
    provider: string;
    result: TestOutcome;
  } | null>(null);

  async function save(provider: string) {
    const key = (drafts[provider] ?? "").trim();
    if (!key) return;
    try {
      await setKey.mutateAsync({ provider, key });
      setDrafts((prev) => ({ ...prev, [provider]: "" }));
      setMessage((prev) => ({ ...prev, [provider]: "Saved to keychain" }));
    } catch (err) {
      setMessage((prev) => ({ ...prev, [provider]: errorMessage(err) }));
    }
  }

  async function test(provider: string) {
    setTesting(provider);
    setOutcome(null);
    try {
      const result = await commands.apiKeyTest(provider, null);
      if (result.status === "ok") {
        setOutcome({ provider, result: result.data });
        setMessage((prev) => ({ ...prev, [provider]: "" }));
      } else {
        setMessage((prev) => ({ ...prev, [provider]: result.error.message }));
      }
    } catch (err) {
      setMessage((prev) => ({ ...prev, [provider]: errorMessage(err) }));
    } finally {
      setTesting(null);
    }
  }

  return (
    <div className="flex flex-col gap-3">
      {statusError && (
        <p className="rounded-md border border-amber-400 p-2 text-xs text-amber-700 dark:text-amber-400">
          Could not read the keychain: {errorMessage(statusError)}. Stored keys
          may still work — this only affects the status display.
        </p>
      )}
      {LLM_PROVIDERS.map((provider) => {
        const configured = status?.find(
          (entry) => entry.provider === provider.id,
        )?.configured;
        return (
          <div
            key={provider.id}
            className="flex flex-col gap-1.5 rounded-md border border-neutral-200 p-2.5 dark:border-neutral-700"
          >
            <div className="flex items-center gap-2 text-sm">
              <KeyRound size={14} className="text-neutral-400" />
              <span className="font-medium">{provider.label}</span>
              <span
                className={`flex items-center gap-1 text-xs ${configured ? "text-green-700 dark:text-green-400" : "text-neutral-400"}`}
              >
                {configured && <Check size={12} />}
                {configured ? "Key stored" : "No key"}
              </span>
            </div>
            <div className="flex gap-1.5">
              <input
                type="password"
                value={drafts[provider.id] ?? ""}
                placeholder={configured ? "Replace key…" : "Paste API key…"}
                autoComplete="off"
                onChange={(event) =>
                  setDrafts((prev) => ({
                    ...prev,
                    [provider.id]: event.target.value,
                  }))
                }
                onKeyDown={(event) => {
                  if (event.key === "Enter") void save(provider.id);
                }}
                className="w-full rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
              />
              <button
                type="button"
                onClick={() => void save(provider.id)}
                className="shrink-0 rounded-md border border-neutral-300 px-2.5 py-1 text-xs dark:border-neutral-600"
              >
                Save
              </button>
              {configured && (
                <>
                  <button
                    type="button"
                    title={`Delete ${provider.label} key`}
                    onClick={() => {
                      if (
                        window.confirm(
                          `Delete the stored ${provider.label} key? Research will stop working until a new key is saved.`,
                        )
                      ) {
                        deleteKey.mutate(provider.id);
                      }
                    }}
                    className="shrink-0 rounded-md border border-neutral-300 px-2 py-1 dark:border-neutral-600"
                  >
                    <Trash2 size={13} />
                  </button>
                  <button
                    type="button"
                    onClick={() => void test(provider.id)}
                    disabled={testing !== null}
                    className="flex shrink-0 items-center gap-1 rounded-md border border-neutral-300 px-2.5 py-1 text-xs disabled:opacity-40 dark:border-neutral-600"
                  >
                    {testing === provider.id ? (
                      <Loader2 size={13} className="animate-spin" />
                    ) : (
                      <FlaskConical size={13} />
                    )}
                    Test
                  </button>
                </>
              )}
            </div>
            {message[provider.id] && (
              <p className="text-xs text-red-600">{message[provider.id]}</p>
            )}
            {outcome?.provider === provider.id && (
              <div className="rounded-md bg-neutral-100 p-2 text-xs dark:bg-neutral-800">
                <p>
                  <span className="font-medium">Reply:</span>{" "}
                  {outcome.result.text || "(empty)"}
                </p>
                <p className="text-neutral-500">
                  {outcome.result.tokens_in} in / {outcome.result.tokens_out}{" "}
                  out tokens
                  {outcome.result.estimated ? " (estimated)" : ""} · usage
                  logged
                  {outcome.result.budget_warning
                    ? ` · ${outcome.result.budget_warning}`
                    : ""}
                </p>
              </div>
            )}
          </div>
        );
      })}
      <p className="text-xs text-neutral-500">
        Keys live in the OS keychain and are never shown again after saving.
      </p>
    </div>
  );
}

function Budgets() {
  const { data: summary } = useTokenSummary();
  const setSetting = useSetSetting();
  const { data: daily } = useSetting("budget.daily_tokens");
  const { data: monthly } = useSetting("budget.monthly_tokens");
  const [dailyDraft, setDailyDraft] = useState<string | null>(null);
  const [monthlyDraft, setMonthlyDraft] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  async function save() {
    setStatus(null);
    try {
      await setSetting.mutateAsync({
        key: "budget.daily_tokens",
        value: dailyDraft ?? daily ?? "0",
      });
      await setSetting.mutateAsync({
        key: "budget.monthly_tokens",
        value: monthlyDraft ?? monthly ?? "0",
      });
      setDailyDraft(null);
      setMonthlyDraft(null);
      setStatus("Saved");
    } catch (err) {
      setStatus(errorMessage(err));
    }
  }

  return (
    <div className="flex flex-col gap-2 text-sm">
      <div className="flex flex-wrap gap-3">
        <label className="flex flex-1 flex-col gap-1">
          <span className="font-medium">Daily cap (tokens, 0 = unlimited)</span>
          <input
            value={dailyDraft ?? daily ?? "0"}
            inputMode="numeric"
            onChange={(event) => setDailyDraft(event.target.value)}
            className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
          />
        </label>
        <label className="flex flex-1 flex-col gap-1">
          <span className="font-medium">
            Monthly cap (tokens, 0 = unlimited)
          </span>
          <input
            value={monthlyDraft ?? monthly ?? "0"}
            inputMode="numeric"
            onChange={(event) => setMonthlyDraft(event.target.value)}
            className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm outline-none dark:border-neutral-600"
          />
        </label>
      </div>
      <button
        type="button"
        onClick={() => void save()}
        className="w-fit rounded-md border border-neutral-300 px-3 py-1 text-xs dark:border-neutral-600"
      >
        Save budgets
      </button>
      {status && <p className="text-xs text-red-600">{status}</p>}
      {summary && (
        <div className="text-xs text-neutral-500">
          <p>
            Today: {summary.day_in + summary.day_out} tokens
            {summary.daily_limit ? ` of ${summary.daily_limit}` : " (no cap)"}
          </p>
          <p>
            Last 30 days: {summary.month_in + summary.month_out} tokens
            {summary.monthly_limit
              ? ` of ${summary.monthly_limit}`
              : " (no cap)"}
          </p>
          {summary.budget_warning && (
            <p className="text-amber-600">{summary.budget_warning}</p>
          )}
        </div>
      )}
    </div>
  );
}

function CacheManager() {
  const { data: stats } = useCacheStats();
  const clearCache = useClearCache();
  const [status, setStatus] = useState<string | null>(null);

  const types = [
    { id: "search", label: "Search results" },
    { id: "webpage", label: "Clipped pages" },
    { id: "llm_response", label: "Model responses" },
    { id: "embedding", label: "Embeddings" },
  ];

  async function clear(entryType: string | null) {
    setStatus(null);
    try {
      await clearCache.mutateAsync(entryType);
      setStatus("Cleared");
    } catch (err) {
      setStatus(errorMessage(err));
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  return (
    <div className="flex flex-col gap-2 text-sm">
      <p className="text-xs text-neutral-500">
        {stats
          ? `${stats.entries} entries · ${formatBytes(stats.bytes)}`
          : "Loading usage…"}
      </p>
      <div className="flex flex-wrap gap-1.5">
        <button
          type="button"
          onClick={() => void clear(null)}
          className="rounded-md border border-neutral-300 px-2.5 py-1 text-xs dark:border-neutral-600"
        >
          Clear all
        </button>
        {types.map((type) => (
          <button
            key={type.id}
            type="button"
            title={`Clear ${type.label}`}
            onClick={() => void clear(type.id)}
            className="rounded-md border border-neutral-300 px-2.5 py-1 text-xs dark:border-neutral-600"
          >
            {type.label}
          </button>
        ))}
      </div>
      {status && <p className="text-xs text-neutral-500">{status}</p>}
    </div>
  );
}

/** Embedding model selection, coverage, and manual re-embed. */
function EmbeddingPanel() {
  const { data: status } = useEmbeddingStatus();
  const setModel = useSetEmbeddingModel();
  const reindex = useReindexEmbeddings();
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const total = status?.total ?? 0;
  const coverage =
    status && total > 0 ? Math.round((status.embedded / total) * 100) : 0;
  const active = status?.model ?? EMBEDDING_MODELS[0].id;
  const customModel = !EMBEDDING_MODELS.some((entry) => entry.id === active);

  async function change(model: string) {
    setError(null);
    setMessage(null);
    try {
      const next = await setModel.mutateAsync(model);
      setMessage(
        next.available
          ? `Switched to ${next.model}; re-embedding in the background.`
          : `Saved ${next.model}, but the embedding backend is not installed.`,
      );
    } catch (err) {
      setError(errorMessage(err));
    }
  }

  return (
    <div className="flex flex-col gap-2 text-sm">
      <div className="flex flex-col gap-1">
        <span className="font-medium">Embedding model</span>
        <Select
          ariaLabel="Embedding model"
          value={active}
          onChange={(value) => void change(value)}
          options={[
            ...EMBEDDING_MODELS.map((entry) => ({
              value: entry.id,
              label: entry.label,
            })),
            ...(customModel ? [{ value: active, label: active }] : []),
          ]}
        />
      </div>

      {status && !status.available && (
        <p className="rounded-md border border-amber-400 p-2 text-xs text-amber-700 dark:text-amber-400">
          The embedding backend is not installed, so auto-linking stays off.
          Install it with{" "}
          <code>uv pip install -r sidecar/requirements-embeddings.txt</code> and
          restart the app.
        </p>
      )}

      {status && (
        <div className="flex flex-col gap-1 text-xs text-neutral-500">
          <p>
            {status.model} · {status.dim} dims
          </p>
          <div className="h-1.5 w-full overflow-hidden rounded-full bg-neutral-200 dark:bg-neutral-700">
            <div
              className="h-full rounded-full bg-neutral-500"
              style={{ width: `${coverage}%` }}
            />
          </div>
          <p>
            {status.embedded} of {status.total} notes embedded ({coverage}%)
          </p>
        </div>
      )}

      <button
        type="button"
        onClick={() => {
          setError(null);
          setMessage(null);
          reindex.mutate(undefined, {
            onSuccess: () => setMessage("Re-embed queued."),
            onError: (err) => setError(errorMessage(err)),
          });
        }}
        className="flex w-fit items-center gap-1 rounded-md border border-neutral-300 px-2.5 py-1 text-xs dark:border-neutral-600"
      >
        <RefreshCw size={13} />
        Re-embed missing notes
      </button>

      {message && (
        <p className="text-xs text-green-700 dark:text-green-400">{message}</p>
      )}
      {error && <p className="text-xs text-red-600">{error}</p>}
    </div>
  );
}

/** Read-only DuckDB status for diagnostics. */
function AnalyticsPanel() {
  const { data: status } = useAnalyticsStatus();
  const [showPath, setShowPath] = useState(false);
  const fileName = status?.path.split(/[\\/]/).pop();
  return (
    <div className="flex flex-col gap-1 text-xs text-neutral-500">
      <p className="flex items-center gap-1">
        <Database size={12} />
        {status?.available ? "DuckDB available" : "DuckDB unavailable"}
      </p>
      {status && (
        <button
          type="button"
          onClick={() => setShowPath((value) => !value)}
          title={showPath ? "Hide full path" : "Show full path"}
          className="w-fit text-left underline decoration-dotted underline-offset-2 hover:text-neutral-700 dark:hover:text-neutral-300"
        >
          {fileName}
        </button>
      )}
      {status && showPath && (
        <p className="break-all text-neutral-400">{status.path}</p>
      )}
      {status && <p>{status.chunks} corpus chunks stored</p>}
    </div>
  );
}

/** Settings tab: providers, keys, models, embedding, cache, budgets. */
function ProjectMemoryList({
  project,
  renderMemory,
}: {
  project: Project;
  renderMemory: (memory: MemoryEntry) => ReactNode;
}) {
  const list = useMemoryList(project.id);
  return (
    <div className="flex flex-col gap-1">
      <span className="text-xs font-medium">{project.name}</span>
      {(list.data ?? []).length === 0 && (
          <span className="text-xs text-neutral-500">None</span>
        )}
        {(list.data ?? []).map(renderMemory)}
    </div>
  );
}

function MemoryPanel() {
  const projects = useProjects(true).data ?? [];
  const globalMemories = useMemoryList(null);
  const forgotten = useForgottenMemories();
  const enabledQuery = useMemoryEnabled();
  const setEnabled = useSetMemoryEnabled();
  const createMemory = useCreateMemory();
  const forgetMemory = useForgetMemory();
  const restoreMemory = useRestoreMemory();
  const deleteMemory = useDeleteMemory();

  const [selectedProject, setSelectedProject] = useState<string | null>(null);
  const [newContent, setNewContent] = useState("");
  const [newScope, setNewScope] = useState<"global" | "project">("global");
  const [showIds, setShowIds] = useState(false);

  const enabled = enabledQuery.data ?? true;

  async function handleCreate() {
    if (!newContent.trim()) return;
    await createMemory.mutateAsync({
      memoryType: newScope,
      projectId: newScope === "project" ? selectedProject : null,
      content: newContent.trim(),
      expiresAt: null,
    });
    setNewContent("");
  }

  function renderMemory(memory: MemoryEntry) {
    return (
      <div key={memory.id} className="flex items-center gap-2 text-sm">
        <span className="flex-1">{memory.content}</span>
        {showIds && (
          <span className="text-xs text-neutral-400">{memory.id}</span>
        )}
        <button
          type="button"
          onClick={() => {
            if (window.confirm("Forget this memory temporarily?")) {
              void forgetMemory.mutate(memory.id);
            }
          }}
          className="text-xs text-neutral-500"
        >
          Forget
        </button>
        <button
          type="button"
          onClick={() => {
            if (window.confirm("Delete this memory permanently?")) {
              void deleteMemory.mutate(memory.id);
            }
          }}
          className="text-xs text-red-500"
        >
          Delete
        </button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <label className="flex items-center gap-2 text-sm">
        <input
          type="checkbox"
          checked={enabled}
          onChange={() => void setEnabled.mutate(!enabled)}
        />
        Memory enabled
      </label>
      <label className="flex items-center gap-2 text-xs text-neutral-500">
        <input
          type="checkbox"
          checked={showIds}
          onChange={() => setShowIds((v) => !v)}
        />
        Show memory IDs
      </label>
      <div className="flex flex-col gap-2">
        <span className="text-xs font-medium">New memory</span>
        <Select
          ariaLabel="Memory scope"
          value={newScope}
          onChange={(value) => setNewScope(value as "global" | "project")}
          options={[
            { value: "global", label: "Global" },
            { value: "project", label: "Project" },
          ]}
        />
        {newScope === "project" && (
          <Select
            ariaLabel="Project"
            value={selectedProject ?? ""}
            onChange={(value) => setSelectedProject(value || null)}
            options={[
              { value: "", label: "Select project" },
              ...projects.map((project) => ({
                value: project.id,
                label: `${project.name} (${project.id})`,
              })),
            ]}
          />
        )}
        <textarea
          value={newContent}
          onChange={(e) => setNewContent(e.target.value)}
          placeholder="Memory content"
          className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm dark:border-neutral-600"
        />
        <button
          type="button"
          onClick={() => void handleCreate()}
          className="rounded-md border border-neutral-300 px-2 py-1 text-sm"
        >
          Save memory
        </button>
      </div>
      <div className="flex flex-col gap-1">
        <span className="text-xs font-medium">Global memories</span>
        {(globalMemories.data ?? []).length === 0 && (
          <span className="text-xs text-neutral-500">None</span>
        )}
        {(globalMemories.data ?? []).map(renderMemory)}
      </div>
      {projects.map((project) => (
        <ProjectMemoryList
          key={project.id}
          project={project}
          renderMemory={renderMemory}
        />
      ))}
      <div className="flex flex-col gap-1">
        <span className="text-xs font-medium">Forgotten memories</span>
        {(forgotten.data ?? []).length === 0 && (
          <span className="text-xs text-neutral-500">None</span>
        )}
        {(forgotten.data ?? []).map((memory) => (
          <div key={memory.id} className="flex items-center gap-2 text-sm">
            <span className="flex-1">{memory.content}</span>
            {showIds && (
              <span className="text-xs text-neutral-400">{memory.id}</span>
            )}
            <button
              type="button"
              onClick={() => void restoreMemory.mutate(memory.id)}
              className="text-xs text-neutral-500"
            >
              Restore
            </button>
            <button
              type="button"
              onClick={() => {
                if (window.confirm("Delete this memory permanently?")) {
                  void deleteMemory.mutate(memory.id);
                }
              }}
              className="text-xs text-red-500"
            >
              Delete
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

export function SettingsView() {
  const { data: provider } = useSetting("llm.provider");
  const setSetting = useSetSetting();
  const current = provider || "openai";

  return (
    <div className="mx-auto flex min-h-0 w-full max-w-2xl flex-1 flex-col gap-3 overflow-y-auto pb-4">
      <Section title="Language Model">
        <div className="flex flex-col gap-1 text-sm">
          <span className="font-medium">Provider</span>
          <Select
            ariaLabel="Provider"
            value={current}
            onChange={(value) => {
              void setSetting.mutateAsync({
                key: "llm.provider",
                value,
              });
            }}
            options={LLM_PROVIDERS.map((entry) => ({
              value: entry.id,
              label: entry.label,
            }))}
          />
        </div>
        <SettingField
          settingKey="llm.model"
          label="Model (blank uses the provider default)"
          placeholder={
            LLM_PROVIDERS.find((entry) => entry.id === current)?.defaultModel
          }
        />
        {current === "openai" && (
          <SettingField
            settingKey="llm.base_url"
            label="Custom endpoint (OpenAI-compatible servers)"
            hint="Blank means api.openai.com. Points research and skills at any compatible server."
            placeholder="https://api.openai.com/v1"
          />
        )}
      </Section>

      <Section title="API Keys">
        <ProviderKeys />
      </Section>

      <Section title="Embedding">
        <EmbeddingPanel />
        <SettingField
          settingKey="linking.threshold"
          label="Auto-link threshold (0–1)"
          hint="Cosine similarity a suggested note must reach. 0.75 is strict for MiniLM-sized models; 0.5 surfaces more."
          placeholder="0.75"
        />
      </Section>

      <Section title="Analytics">
        <AnalyticsPanel />
      </Section>

      <Section title="Cache">
        <SettingField
          settingKey="cache.max_bytes"
          label="Maximum cache size (bytes)"
          hint="Least-recently-used entries are evicted past this size. Default 1073741824 (1 GB)."
          placeholder="1073741824"
        />
        <CacheManager />
      </Section>

      <Section title="Token Budgets">
        <Budgets />
      </Section>

      <Section title="Memories">
        <MemoryPanel />
      </Section>
    </div>
  );
}
