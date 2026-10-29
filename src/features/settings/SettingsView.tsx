import { useState, useEffect } from "react";
import { createPortal } from "react-dom";
import type { ReactNode } from "react";
import {
  BarChart3,
  Brain,
  Check,
  Cog,
  Database,
  FlaskConical,
  HardDrive,
  KeyRound,
  Loader2,
  RefreshCw,
  Sparkles,
  Search,
  Volume2,
  Wallet,
  X,
  ArrowUp,
  ArrowDown,
} from "lucide-react";
import { commands } from "../../ipc/bindings";
import type { MemoryEntry, Project, TestOutcome } from "../../ipc/bindings";
import {
  errorMessage,
  useAnalyticsStatus,
  useCacheStats,
  useClearCache,
  useEmbeddingStatus,
  useKeyStatus,
  useLlmCatalog,
  useLlmConfig,
  useMemoryEnabled,
  useMemoryList,
  useForgottenMemories,
  useCreateMemory,
  useForgetMemory,
  useRestoreMemory,
  useDeleteMemory,
  useSetMemoryEnabled,
  useSetMemoryScope,
  useReindexEmbeddings,
  useSetEmbeddingModel,
  useSetKey,
  useProjects,
  useSetSetting,
  useSetting,
  useTokenSummary,
} from "../../ipc/queries";
import { Select } from "../../components/Select";
import { useUi } from "../../stores/ui";
import { ProviderKeyRow, Section, SettingField } from "./primitives";
import { AudioSection } from "./AudioSection";
import { ModelsSection } from "./ModelsSection";

/// Search backends available for the fallback chain. `ddgs` is always present
/// (keyless). You.com falls back from its API (Python sidecar) to an MCP call
/// (Rust) transparently. Others require a stored key.
type SearchProvider = {
  id: string;
  label: string;
  requiresKey: boolean;
  keyId?: string;
  needsUrl?: boolean;
};

const SEARCH_PROVIDERS: SearchProvider[] = [
  { id: "ddgs", label: "DuckDuckGo (DDGS)", requiresKey: false },
  { id: "ddg_lite", label: "DuckDuckGo Lite", requiresKey: false },
  { id: "ddg_html", label: "DuckDuckGo HTML", requiresKey: false },
  { id: "searxng", label: "SearXNG (self-hosted)", requiresKey: false, needsUrl: true },
  { id: "brave", label: "Brave Search", requiresKey: true, keyId: "brave" },
  { id: "firecrawl", label: "Firecrawl", requiresKey: true, keyId: "firecrawl" },
  { id: "keenable", label: "Keenable", requiresKey: true, keyId: "keenable" },
  { id: "exa", label: "Exa", requiresKey: true, keyId: "exa" },
  { id: "tavily", label: "Tavily", requiresKey: true, keyId: "tavily" },
  { id: "serpbase", label: "SerpBase", requiresKey: true, keyId: "serpbase" },
  { id: "bing", label: "Bing", requiresKey: true, keyId: "bing" },
  { id: "parallel", label: "Parallel", requiresKey: true, keyId: "parallel" },
  { id: "anysearch", label: "AnySearch AI", requiresKey: true, keyId: "anysearch" },
  { id: "perplexity", label: "Perplexity", requiresKey: true, keyId: "perplexity" },
  { id: "you", label: "You.com", requiresKey: true, keyId: "you" },
];

const DEFAULT_SEARCH_PRIORITY: string[] = [
  "ddgs", "firecrawl", "keenable", "exa", "tavily",
  "searxng", "brave", "parallel", "anysearch", "perplexity",
  "serpbase", "bing", "ddg_lite", "ddg_html", "you",
];

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


/** LLM provider keys. Model selection lives in the Models tab. */
function ProviderKeys() {
  const { data: catalog = [] } = useLlmCatalog();
  const { data: config } = useLlmConfig();
  const [testing, setTesting] = useState<string | null>(null);
  const [message, setMessage] = useState<Record<string, string>>({});
  const [outcome, setOutcome] = useState<{
    provider: string;
    result: TestOutcome;
  } | null>(null);

  const providers = [
    ...catalog.map((entry) => ({ id: entry.id, label: entry.label })),
    ...(config?.custom_providers ?? []).map((custom) => ({
      id: custom.id,
      label: custom.display_name,
    })),
  ];

  async function test(provider: string) {
    setTesting(provider);
    setOutcome(null);
    setMessage((current) => ({ ...current, [provider]: "" }));
    try {
      const result = await commands.apiKeyTest(provider, null);
      if (result.status === "ok") {
        setOutcome({ provider, result: result.data });
      } else {
        setMessage((current) => ({
          ...current,
          [provider]: result.error.message,
        }));
      }
    } catch (err) {
      setMessage((current) => ({
        ...current,
        [provider]: errorMessage(err),
      }));
    } finally {
      setTesting(null);
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-neutral-500">
        Keys live in the OS keychain and are never shown again after saving.
        Each provider can hold several models; manage the fallback chain in the
        Models tab.
      </p>
      {providers.map((provider) => (
        <div
          key={provider.id}
          className="flex flex-col gap-1.5 rounded-md border border-neutral-200 p-2.5 dark:border-neutral-700"
        >
          <div className="flex items-center justify-between gap-2">
            <span className="text-sm font-medium">{provider.label}</span>
            <button
              type="button"
              onClick={() => void test(provider.id)}
              disabled={testing !== null}
              className="flex shrink-0 items-center gap-1 rounded-md border border-border px-2.5 py-1 text-xs disabled:opacity-40"
            >
              {testing === provider.id ? (
                <Loader2 size={13} className="animate-spin" />
              ) : (
                <FlaskConical size={13} />
              )}
              Test
            </button>
          </div>
          <ProviderKeyRow provider={provider.id} label={provider.label} />
          {message[provider.id] && (
            <p className="text-xs text-red-600">{message[provider.id]}</p>
          )}
          {outcome && outcome.provider === provider.id && (
            <div className="rounded-md bg-neutral-100 p-2 text-xs dark:bg-neutral-800">
              <p>
                <span className="font-medium">Reply:</span>{" "}
                {outcome.result.text || "(empty)"}
              </p>
              <p className="text-neutral-500">
                {outcome.result.provider} · {outcome.result.model} ·{" "}
                {outcome.result.tokens_in} in / {outcome.result.tokens_out} out
                tokens
                {outcome.result.estimated ? " (estimated)" : ""} · usage logged
                {outcome.result.budget_warning
                  ? ` · ${outcome.result.budget_warning}`
                  : ""}
              </p>
            </div>
          )}
        </div>
      ))}
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
            className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
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
            className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
          />
        </label>
      </div>
      <button
        type="button"
        onClick={() => void save()}
        className="w-fit rounded-md border border-border px-3 py-1 text-xs"
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

/** Manages the ordered fallback chain and safe-search preference. */
function SearchProvidersSection() {
  const { data: status } = useKeyStatus();
  const { data: priority } = useSetting("research.providers");
  const { data: safeSearch } = useSetting("research.safe_search");
  const { data: searxngUrl } = useSetting("research.searxng_url");
  
  const setSetting = useSetSetting();
  const setKey = useSetKey();
  
  const [draftPriority, setDraftPriority] = useState<string[] | null>(null);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);
  const [keyDrafts, setKeyDrafts] = useState<Record<string, string>>({});

  const ordered: string[] = (() => {
    const stored = priority
      ? (JSON.parse(priority) as string[]).filter((id) =>
          SEARCH_PROVIDERS.some((p) => p.id === id),
        )
      : [];
    const seen = new Set(stored);
    const missing = DEFAULT_SEARCH_PRIORITY.filter((id) => !seen.has(id));
    return [...stored, ...missing];
  })();

  const effectivePriority = draftPriority ?? ordered;

  const labelFor = (id: string) =>
    SEARCH_PROVIDERS.find((p) => p.id === id)?.label ?? id;

  const hasKey = (provider: (typeof SEARCH_PROVIDERS)[number]) => {
    if (!provider.requiresKey) return true;
    if (!provider.keyId) return true;
    return status?.find((entry) => entry.provider === provider.keyId)?.configured ?? false;
  };

  async function savePriority() {
    setStatusMsg(null);
    try {
      await setSetting.mutateAsync({
        key: "research.providers",
        value: JSON.stringify(effectivePriority),
      });
      setDraftPriority(null);
      setStatusMsg("Saved");
    } catch (err) {
      setStatusMsg(errorMessage(err));
    }
  }

  async function resetPriority() {
    setDraftPriority(null);
    setStatusMsg(null);
    try {
      await setSetting.mutateAsync({
        key: "research.providers",
        value: JSON.stringify(DEFAULT_SEARCH_PRIORITY),
      });
      setStatusMsg("Reset to defaults");
    } catch (err) {
      setStatusMsg(errorMessage(err));
    }
  }

  async function setSafeSearch(value: string) {
    try {
      await setSetting.mutateAsync({ key: "research.safe_search", value });
    } catch (err) {
      setStatusMsg(errorMessage(err));
    }
  }

  async function saveKey(providerId: string) {
    const key = (keyDrafts[providerId] ?? "").trim();
    if (!key) return;
    try {
      await setKey.mutateAsync({ provider: providerId, key });
      setKeyDrafts((prev) => ({ ...prev, [providerId]: "" }));
    } catch (err) {
      alert(errorMessage(err));
    }
  }

  function moveUp(index: number) {
    if (index === 0) return;
    const current = draftPriority ?? [...ordered];
    const next = [...current];
    [next[index - 1], next[index]] = [next[index], next[index - 1]];
    setDraftPriority(next);
  }

  function moveDown(index: number) {
    const current = draftPriority ?? [...ordered];
    if (index === current.length - 1) return;
    const next = [...current];
    [next[index + 1], next[index]] = [next[index], next[index + 1]];
    setDraftPriority(next);
  }

  return (
    <div className="flex flex-col gap-3 text-sm">
      <div className="flex items-center justify-between gap-2">
        <span className="font-medium">Fallback priority (top = tried first)</span>
        <button
          type="button"
          onClick={() => void resetPriority()}
          className="rounded-md border border-border px-2 py-1 text-xs"
        >
          Reset
        </button>
      </div>
      <p className="text-xs text-neutral-500">
        Use the up/down arrows to reorder providers. The first provider with results wins.
      </p>
      
      <div className="space-y-1">
        {effectivePriority.map((id, index) => {
          const provider = SEARCH_PROVIDERS.find((p) => p.id === id);
          const configured = provider ? hasKey(provider) : false;
          
          return (
            <div
              key={id}
              className="flex flex-col gap-2 rounded-md border border-border p-2 bg-transparent"
            >
              <div className="flex items-center gap-2">
                {/* UP / DOWN ARROWS */}
                <div className="flex flex-col -my-1">
                  <button 
                    onClick={() => moveUp(index)} 
                    disabled={index === 0}
                    className="text-neutral-500 hover:text-white disabled:opacity-20"
                  >
                    <ArrowUp size={14} />
                  </button>
                  <button 
                    onClick={() => moveDown(index)} 
                    disabled={index === effectivePriority.length - 1}
                    className="text-neutral-500 hover:text-white disabled:opacity-20"
                  >
                    <ArrowDown size={14} />
                  </button>
                </div>

                <span className="flex-1 ml-2">{labelFor(id)}</span>
                
                {/* GREEN CHECKMARK OR KEY NEEDED */}
                {configured && provider?.requiresKey && (
                  <span className="flex items-center gap-1 text-xs text-green-700 dark:text-green-400">
                    <Check size={12} /> Key stored
                  </span>
                )}
                {!configured && provider?.requiresKey && (
                  <span className="text-xs text-amber-600">Key needed</span>
                )}
                {provider?.needsUrl && id === 'searxng' && !searxngUrl && (
                  <span className="text-xs text-amber-600">URL needed</span>
                )}
              </div>
              
              {/* RENDER API KEY INPUT IF NEEDED */}
              {provider?.requiresKey && provider.keyId && !configured && (
                <div className="flex gap-2 pl-6">
                  <input
                    type="password"
                    placeholder={`Paste ${labelFor(id)} API Key...`}
                    value={keyDrafts[provider.keyId] ?? ""}
                    onChange={(e) => setKeyDrafts(prev => ({ ...prev, [provider.keyId!]: e.target.value }))}
                    className="w-full rounded-md border border-border bg-black px-2 py-1 text-xs"
                  />
                  <button
                    onClick={() => void saveKey(provider.keyId!)}
                    className="rounded-md border border-border px-2 py-1 text-xs hover:bg-white/10"
                  >
                    Save
                  </button>
                </div>
              )}

              {/* RENDER SEARXNG URL INPUT */}
              {id === 'searxng' && (
                <div className="pl-6">
                  <input
                    type="text"
                    placeholder="https://searxng.example.com"
                    defaultValue={searxngUrl ?? ""}
                    onBlur={(e) => void setSetting.mutateAsync({ key: "research.searxng_url", value: e.target.value })}
                    className="w-full rounded-md border border-border bg-transparent px-2 py-1 text-xs"
                  />
                </div>
              )}
            </div>
          );
        })}
      </div>

      <div className="flex items-center gap-2 mt-4">
        <label className="text-sm">Safe search</label>
        <Select
          ariaLabel="Safe search"
          value={safeSearch ?? "off"}
          onChange={(value) => void setSafeSearch(value)}
          options={[
            { value: "off", label: "Off" },
            { value: "moderate", label: "Moderate" },
            { value: "strict", label: "Strict" },
          ]}
          className="w-36"
        />
      </div>
      
      <div className="flex gap-2">
        <button
          type="button"
          onClick={() => void savePriority()}
          disabled={draftPriority === null}
          className="rounded-md border border-border px-2.5 py-1 text-xs disabled:opacity-40"
        >
          Save order
        </button>
        {statusMsg && (
          <span className={`text-xs ${statusMsg.includes("Saved") || statusMsg.includes("Reset") ? "text-green-500" : "text-red-600"}`}>
            {statusMsg}
          </span>
        )}
      </div>
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
          ? `${stats.entries} entries - ${formatBytes(stats.bytes)}`
          : "Loading usageâ€¦"}
      </p>
      <div className="flex flex-wrap gap-1.5">
        <button
          type="button"
          onClick={() => void clear(null)}
          className="rounded-md border border-border px-2.5 py-1 text-xs"
        >
          Clear all
        </button>
        {types.map((type) => (
          <button
            key={type.id}
            type="button"
            title={`Clear ${type.label}`}
            onClick={() => void clear(type.id)}
            className="rounded-md border border-border px-2.5 py-1 text-xs"
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
            {status.model} - {status.dim} dims
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
        className="flex w-fit items-center gap-1 rounded-md border border-border px-2.5 py-1 text-xs"
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
  const setMemoryScope = useSetMemoryScope();
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
        <Select
          ariaLabel="Memory scope"
          size="sm"
          className="w-36"
          value={memory.memory_type === "project" ? (memory.project_id ?? "") : ""}
          onChange={(value) =>
            void setMemoryScope.mutate({
              id: memory.id,
              memoryType: value ? "project" : "global",
              projectId: value || null,
            })
          }
          options={[
            { value: "", label: "Global" },
            ...projects.map((project) => ({
              value: project.id,
              label: project.name,
            })),
          ]}
        />
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
          className="rounded-md border border-border bg-transparent px-2 py-1 text-sm"
        />
        <button
          type="button"
          onClick={() => void handleCreate()}
          className="rounded-md border border-border px-2 py-1 text-sm"
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

type SettingsTabId =
  | "models"
  | "api-keys"
  | "audio"
  | "embedding"
  | "analytics"
  | "cache"
  | "token-budgets"
  | "memories"
  | "search";

const SETTINGS_TABS: { id: SettingsTabId; label: string; icon: typeof Cog }[] =
  [
    { id: "models", label: "Models", icon: Sparkles },
    { id: "api-keys", label: "API Keys", icon: KeyRound },
    { id: "audio", label: "Audio", icon: Volume2 },
    { id: "embedding", label: "Embedding", icon: Database },
    { id: "analytics", label: "Analytics", icon: BarChart3 },
    { id: "cache", label: "Cache", icon: HardDrive },
    { id: "token-budgets", label: "Token Budgets", icon: Wallet },
    { id: "memories", label: "Memories", icon: Brain },
    { id: "search", label: "Search", icon: Search },
  ];

/** Renders only the section for the active settings tab. */
function SettingsTabContent({ tab }: { tab: SettingsTabId }) {
  switch (tab) {
    case "models":
      return <ModelsSection />;
    case "audio":
      return <AudioSection />;
    case "api-keys":
      return <ProviderKeys />;
    case "embedding":
      return (
        <Section>
          <EmbeddingPanel />
          <SettingField
            settingKey="linking.threshold"
            label="Auto-link threshold (0â€“1)"
            hint="Cosine similarity a suggested note must reach. 0.75 is strict for MiniLM-sized models; 0.5 surfaces more."
            placeholder="0.75"
          />
        </Section>
      );
    case "analytics":
      return <AnalyticsPanel />;
    case "cache":
      return (
        <Section>
          <SettingField
            settingKey="cache.max_bytes"
            label="Maximum cache size (bytes)"
            hint="Least-recently-used entries are evicted past this size. Default 1073741824 (1 GB)."
            placeholder="1073741824"
          />
          <CacheManager />
        </Section>
      );
    case "token-budgets":
      return <Budgets />;
    case "search":
      return <SearchProvidersSection />;
    case "memories":
      return <MemoryPanel />;
  }
}

/**
 * Settings rendered as a centered modal with a fixed sidebar for tab
 * navigation. The modal is portalled to `document.body` so it floats over the
 * application; the dimmed background stays visible and the navigation tabs
 * remain reachable.
 */
export function SettingsModal() {
  const open = useUi((state) => state.settingsOpen);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);
  const [activeSettingsTab, setActiveSettingsTab] = useState<SettingsTabId>(
    "models",
  );

  useEffect(() => {
    if (!open) return undefined;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        setSettingsOpen(false);
      }
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open, setSettingsOpen]);

  if (!open) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-background/40 pointer-events-none"
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        className="pointer-events-auto flex h-full w-full max-w-4xl max-h-[80vh] flex-col overflow-hidden rounded-lg border border-border bg-card text-foreground shadow-xl"
        onClick={(event) => event.stopPropagation()}
      >
        <header className="flex items-center justify-between p-4">
          <h2 className="text-lg font-semibold">Settings</h2>
          <button
            type="button"
            aria-label="Close settings"
            onClick={() => setSettingsOpen(false)}
            className="rounded p-1 text-neutral-500 hover:bg-neutral-100 hover:text-neutral-900 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
            title="Close (Esc)"
          >
            <X size={16} />
          </button>
        </header>

        <div className="flex flex-1 min-h-0 overflow-hidden">
          <nav
            aria-label="Settings"
            className="flex w-64 shrink-0 flex-col gap-1 overflow-y-hidden border-r border-border p-6"
          >
            {SETTINGS_TABS.map((tab) => {
              const Icon = tab.icon;
              const isActive = tab.id === activeSettingsTab;
              return (
                <button
                  key={tab.id}
                  type="button"
                  onClick={() => setActiveSettingsTab(tab.id)}
                className={`flex items-center gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors ${
                  isActive
                    ? "bg-white/10 text-white font-medium"
                    : "bg-white text-black hover:bg-gray-200"
                }`}
                >
                  <Icon size={14} />
                  <span>{tab.label}</span>
                </button>
              );
            })}
          </nav>

          <div className="flex-1 overflow-y-auto p-6 settings-scroll">
            <SettingsTabContent tab={activeSettingsTab} />
          </div>
        </div>
      </div>
    </div>,
    document.body,
  );
}
