import { useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  FlaskConical,
  Loader2,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";
import type {
  CustomProvider,
  LlmConfig,
  ProviderDescriptor,
  TestOutcome,
} from "../../ipc/bindings";
import { commands } from "../../ipc/bindings";
import {
  errorMessage,
  useKeyStatus,
  useLlmCatalog,
  useLlmConfig,
  useSaveLlmConfig,
  useSetKey,
  useSyncModels,
} from "../../ipc/queries";
import { Select } from "../../components/Select";
import { ProviderKeyRow, Section } from "./primitives";

/** Turns a display name into a keychain-safe `custom.<slug>` id. */
function customIdFromName(name: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-")
    .slice(0, 40)
    .replace(/-+$/g, "");
  return `custom.${slug || "endpoint"}`;
}

function splitList(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

/** All known model ids for a provider: stored, catalog suggestions, custom. */
function modelsFor(
  provider: string,
  catalog: ProviderDescriptor[],
  config: LlmConfig,
): string[] {
  const models = new Set<string>(config.provider_models[provider] ?? []);
  catalog
    .find((entry) => entry.id === provider)
    ?.models.forEach((model) => models.add(model));
  config.custom_providers
    .find((custom) => custom.id === provider)
    ?.models.forEach((model) => models.add(model));
  return [...models].sort();
}

function providerLabel(
  id: string,
  catalog: ProviderDescriptor[],
  config: LlmConfig,
): string {
  return (
    config.custom_providers.find((custom) => custom.id === id)?.display_name ||
    catalog.find((entry) => entry.id === id)?.label ||
    id
  );
}

/**
 * Model management: the ordered fallback chain, per-provider model lists and
 * keys, and user-defined OpenAI-compatible endpoints.
 *
 * Edits are local until "Save model settings" so a half-typed endpoint cannot
 * become the active model. Keys are the exception: they go to the OS keychain
 * immediately and are never returned.
 */
export function ModelsSection() {
  const { data: catalog = [] } = useLlmCatalog();
  const { data: stored, isLoading } = useLlmConfig();
  const { data: keyStatus } = useKeyStatus();
  const save = useSaveLlmConfig();
  const sync = useSyncModels();
  const setKey = useSetKey();
  const [draft, setDraft] = useState<LlmConfig | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [busyProvider, setBusyProvider] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<{
    provider: string;
    result: TestOutcome;
  } | null>(null);

  const [addProvider, setAddProvider] = useState("");
  const [addModel, setAddModel] = useState("");
  const [modelDrafts, setModelDrafts] = useState<Record<string, string>>({});

  const [customName, setCustomName] = useState("");
  const [customId, setCustomId] = useState("");
  const [customIdTouched, setCustomIdTouched] = useState(false);
  const [customUrl, setCustomUrl] = useState("");
  const [customModels, setCustomModels] = useState("");
  const [customKey, setCustomKey] = useState("");
  const [customMessage, setCustomMessage] = useState<string | null>(null);

  if (isLoading || !stored) {
    return (
      <Section>
        <p className="flex items-center gap-2 text-sm text-neutral-500">
          <Loader2 size={14} className="animate-spin" /> Loading model settings…
        </p>
      </Section>
    );
  }

  const config: LlmConfig = draft ?? stored;
  const allProviders = [
    ...catalog.map((entry) => ({ id: entry.id, label: entry.label })),
    ...config.custom_providers.map((custom) => ({
      id: custom.id,
      label: custom.display_name,
    })),
  ];
  const providerOptions = allProviders.map((entry) => ({
    value: entry.id,
    label: entry.label,
  }));
  const selectedProvider = addProvider || allProviders[0]?.id || "";
  const suggestions = modelsFor(selectedProvider, catalog, config);

  function update(change: (current: LlmConfig) => LlmConfig) {
    setDraft((current) => change(current ?? config));
    setMessage(null);
  }

  async function saveNow() {
    setMessage(null);
    try {
      await save.mutateAsync(config);
      setDraft(null);
      setMessage("Saved and active");
    } catch (err) {
      setMessage(errorMessage(err));
    }
  }

  function move(index: number, delta: number) {
    update((current) => {
      const chain = [...current.chain];
      const target = index + delta;
      if (target < 0 || target >= chain.length) return current;
      [chain[index], chain[target]] = [chain[target], chain[index]];
      return { ...current, chain };
    });
  }

  function addChainEntry() {
    const model = addModel.trim() || suggestions[0] || "";
    if (!selectedProvider || !model) return;
    update((current) => ({
      ...current,
      chain: [...current.chain, { provider: selectedProvider, model }],
    }));
    setAddModel("");
  }

  function removeChainEntry(index: number) {
    update((current) => ({
      ...current,
      chain: current.chain.filter((_, position) => position !== index),
    }));
  }

  function setBaseUrl(provider: string, value: string) {
    update((current) => ({
      ...current,
      base_urls: { ...current.base_urls, [provider]: value },
    }));
  }

  function addModelToProvider(provider: string) {
    const model = (modelDrafts[provider] ?? "").trim();
    if (!model) return;
    update((current) => {
      const existing = new Set(current.provider_models[provider] ?? []);
      existing.add(model);
      const models = [...existing].sort();
      return {
        ...current,
        provider_models: { ...current.provider_models, [provider]: models },
        custom_providers: current.custom_providers.map((custom) =>
          custom.id === provider ? { ...custom, models } : custom,
        ),
      };
    });
    setModelDrafts((current) => ({ ...current, [provider]: "" }));
  }

  function removeModelFromProvider(provider: string, model: string) {
    update((current) => {
      const models = (current.provider_models[provider] ?? []).filter(
        (entry) => entry !== model,
      );
      return {
        ...current,
        provider_models: { ...current.provider_models, [provider]: models },
        custom_providers: current.custom_providers.map((custom) =>
          custom.id === provider ? { ...custom, models } : custom,
        ),
      };
    });
  }

  async function syncModels(provider: string) {
    setBusyProvider(provider);
    setMessage(null);
    try {
      const models = await sync.mutateAsync(provider);
      update((current) => ({
        ...current,
        provider_models: { ...current.provider_models, [provider]: models },
      }));
      setMessage(`Synced ${models.length} models for ${providerLabel(provider, catalog, config)}`);
    } catch (err) {
      setMessage(`Sync failed: ${errorMessage(err)}`);
    } finally {
      setBusyProvider(null);
    }
  }

  async function testProvider(provider: string) {
    setBusyProvider(provider);
    setOutcome(null);
    setMessage(null);
    const model =
      config.chain.find((entry) => entry.provider === provider)?.model ??
      config.provider_models[provider]?.[0] ??
      null;
    try {
      const result = await commands.apiKeyTest(provider, model);
      if (result.status === "ok") {
        setOutcome({ provider, result: result.data });
      } else {
        setMessage(result.error.message);
      }
    } catch (err) {
      setMessage(errorMessage(err));
    } finally {
      setBusyProvider(null);
    }
  }

  async function addCustomProvider() {
    const name = customName.trim();
    if (!name) {
      setCustomMessage("Give the endpoint a display name.");
      return;
    }
    const id = (customId.trim() || customIdFromName(name)).toLowerCase();
    if (!/^custom\.[a-z0-9]+(?:-[a-z0-9]+)*$/.test(id)) {
      setCustomMessage("Provider id must look like custom.my-endpoint.");
      return;
    }
    const baseUrl = customUrl.trim();
    if (!/^https?:\/\//.test(baseUrl)) {
      setCustomMessage("Base URL must start with http:// or https://.");
      return;
    }
    if (config.custom_providers.some((custom) => custom.id === id)) {
      setCustomMessage("That provider id is already used.");
      return;
    }
    const models = splitList(customModels);
    const provider: CustomProvider = {
      id,
      display_name: name,
      base_url: baseUrl.replace(/\/+$/, ""),
      models,
    };
    if (customKey.trim()) {
      try {
        await setKey.mutateAsync({ provider: id, key: customKey.trim() });
      } catch (err) {
        setCustomMessage(`Could not store the key: ${errorMessage(err)}`);
        return;
      }
    }
    update((current) => ({
      ...current,
      custom_providers: [...current.custom_providers, provider],
      provider_models: { ...current.provider_models, [id]: models },
    }));
    setCustomName("");
    setCustomId("");
    setCustomIdTouched(false);
    setCustomUrl("");
    setCustomModels("");
    setCustomKey("");
    setCustomMessage("Endpoint added. Save to activate it.");
  }

  function updateCustomProvider(
    id: string,
    change: Partial<CustomProvider>,
  ) {
    update((current) => ({
      ...current,
      custom_providers: current.custom_providers.map((custom) =>
        custom.id === id ? { ...custom, ...change } : custom,
      ),
    }));
  }

  function removeCustomProvider(id: string) {
    update((current) => ({
      ...current,
      custom_providers: current.custom_providers.filter(
        (custom) => custom.id !== id,
      ),
      chain: current.chain.filter((entry) => entry.provider !== id),
    }));
  }

  const dirty = draft !== null;

  return (
    <div className="flex flex-col gap-3">
      <Section title="Fallback chain">
        <p className="text-xs text-neutral-500">
          The first entry is the primary model. If it fails (retired model, rate
          limit, outage), EndlessLi tries the next entry and logs usage against
          whichever one answered.
        </p>
        <div className="flex flex-col gap-1">
          {config.chain.map((entry, index) => (
            <div
              key={`${entry.provider}-${entry.model}-${index}`}
              className="flex items-center gap-2 rounded-md border border-border px-2 py-1.5 text-sm"
            >
              <span className="w-5 text-xs text-neutral-500">{index + 1}.</span>
              <span className="flex-1 truncate">
                {providerLabel(entry.provider, catalog, config)}
                <span className="text-neutral-500"> · {entry.model}</span>
              </span>
              <button
                type="button"
                aria-label="Move up"
                onClick={() => move(index, -1)}
                disabled={index === 0}
                className="rounded p-0.5 text-neutral-500 disabled:opacity-20"
              >
                <ArrowUp size={13} />
              </button>
              <button
                type="button"
                aria-label="Move down"
                onClick={() => move(index, 1)}
                disabled={index === config.chain.length - 1}
                className="rounded p-0.5 text-neutral-500 disabled:opacity-20"
              >
                <ArrowDown size={13} />
              </button>
              <button
                type="button"
                aria-label="Remove from chain"
                onClick={() => removeChainEntry(index)}
                className="rounded p-0.5 text-red-500"
              >
                <Trash2 size={13} />
              </button>
            </div>
          ))}
          {config.chain.length === 0 && (
            <p className="text-xs text-amber-600">
              Add at least one model before saving.
            </p>
          )}
        </div>
        <div className="flex flex-col gap-1.5 rounded-md border border-dashed border-border p-2 sm:flex-row">
          <Select
            ariaLabel="Provider to add"
            value={selectedProvider}
            onChange={(value) => {
              setAddProvider(value);
              setAddModel("");
            }}
            options={providerOptions}
            className="sm:w-56"
          />
          <input
            list="llm-chain-models"
            value={addModel}
            placeholder={suggestions[0] ?? "Model id"}
            onChange={(event) => setAddModel(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") addChainEntry();
            }}
            className="w-full rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
          />
          <datalist id="llm-chain-models">
            {suggestions.map((model) => (
              <option key={model} value={model} />
            ))}
          </datalist>
          <button
            type="button"
            onClick={addChainEntry}
            disabled={!selectedProvider}
            className="flex shrink-0 items-center gap-1 rounded-md border border-border px-2.5 py-1 text-xs disabled:opacity-40"
          >
            <Plus size={12} /> Add
          </button>
        </div>
      </Section>

      <Section title="Providers, keys, and models">
        {allProviders.map((provider) => {
          const custom = config.custom_providers.find(
            (entry) => entry.id === provider.id,
          );
          const descriptor = catalog.find((entry) => entry.id === provider.id);
          const models = modelsFor(provider.id, catalog, config);
          const configured =
            keyStatus?.find((entry) => entry.provider === provider.id)
              ?.configured ?? false;
          return (
            <div
              key={provider.id}
              className="flex flex-col gap-2 rounded-md border border-neutral-200 p-2.5 dark:border-neutral-700"
            >
              <div className="flex items-center justify-between gap-2 text-sm">
                <span className="font-medium">{provider.label}</span>
                <div className="flex items-center gap-1">
                  <button
                    type="button"
                    onClick={() => void testProvider(provider.id)}
                    disabled={busyProvider !== null}
                    className="flex items-center gap-1 rounded-md border border-border px-2 py-0.5 text-xs disabled:opacity-40"
                  >
                    {busyProvider === provider.id ? (
                      <Loader2 size={11} className="animate-spin" />
                    ) : (
                      <FlaskConical size={11} />
                    )}
                    Test
                  </button>
                  <button
                    type="button"
                    onClick={() => void syncModels(provider.id)}
                    disabled={busyProvider !== null || !configured}
                    title="Fetch the provider's /models list"
                    className="flex items-center gap-1 rounded-md border border-border px-2 py-0.5 text-xs disabled:opacity-40"
                  >
                    {busyProvider === provider.id ? (
                      <Loader2 size={11} className="animate-spin" />
                    ) : (
                      <RefreshCw size={11} />
                    )}
                    Sync
                  </button>
                </div>
              </div>

              <ProviderKeyRow provider={provider.id} label={provider.label} />

              {custom ? (
                <div className="grid gap-1.5 sm:grid-cols-2">
                  <label className="flex flex-col gap-0.5 text-xs">
                    <span className="text-neutral-500">Display name</span>
                    <input
                      value={custom.display_name}
                      onChange={(event) =>
                        updateCustomProvider(custom.id, {
                          display_name: event.target.value,
                        })
                      }
                      className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
                    />
                  </label>
                  <label className="flex flex-col gap-0.5 text-xs">
                    <span className="text-neutral-500">Base URL</span>
                    <input
                      value={custom.base_url}
                      onChange={(event) =>
                        updateCustomProvider(custom.id, {
                          base_url: event.target.value,
                        })
                      }
                      className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
                    />
                  </label>
                </div>
              ) : (
                <label className="flex flex-col gap-0.5 text-xs">
                  <span className="text-neutral-500">
                    Base URL override (blank uses the default)
                  </span>
                  <input
                    value={config.base_urls[provider.id] ?? ""}
                    placeholder={descriptor?.default_base_url ?? ""}
                    onChange={(event) =>
                      setBaseUrl(provider.id, event.target.value)
                    }
                    className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
                  />
                  {descriptor?.base_url_hint && (
                    <span className="text-[11px] text-neutral-500">
                      {descriptor.base_url_hint}
                    </span>
                  )}
                </label>
              )}

              <div className="flex flex-wrap gap-1">
                {models.map((model) => (
                  <span
                    key={model}
                    className="flex items-center gap-1 rounded-full border border-border px-2 py-0.5 text-[11px]"
                  >
                    {model}
                    <button
                      type="button"
                      aria-label={`Remove ${model}`}
                      onClick={() => removeModelFromProvider(provider.id, model)}
                      className="text-neutral-400 hover:text-red-500"
                    >
                      <Trash2 size={10} />
                    </button>
                  </span>
                ))}
                {models.length === 0 && (
                  <span className="text-[11px] text-neutral-500">
                    No models yet. Sync or add one below.
                  </span>
                )}
              </div>
              <div className="flex gap-1.5">
                <input
                  value={modelDrafts[provider.id] ?? ""}
                  placeholder="Add a model id manually…"
                  onChange={(event) =>
                    setModelDrafts((current) => ({
                      ...current,
                      [provider.id]: event.target.value,
                    }))
                  }
                  onKeyDown={(event) => {
                    if (event.key === "Enter") addModelToProvider(provider.id);
                  }}
                  className="w-full rounded-md border border-border bg-transparent px-2 py-1 text-xs outline-none"
                />
                <button
                  type="button"
                  onClick={() => addModelToProvider(provider.id)}
                  className="shrink-0 rounded-md border border-border px-2 py-1 text-xs"
                >
                  Add model
                </button>
              </div>

              {custom && (
                <div className="flex items-center justify-between gap-2">
                  <span className="font-mono text-[11px] text-neutral-500">
                    {custom.id}
                  </span>
                  <button
                    type="button"
                    onClick={() => removeCustomProvider(custom.id)}
                    className="text-xs text-red-500"
                  >
                    Remove endpoint
                  </button>
                </div>
              )}

              {outcome && outcome.provider === provider.id && (
                <div className="rounded-md bg-neutral-100 p-2 text-xs dark:bg-neutral-800">
                  <p>
                    <span className="font-medium">Reply:</span>{" "}
                    {outcome.result.text || "(empty)"}
                  </p>
                  <p className="text-neutral-500">
                    {outcome.result.provider} · {outcome.result.model} ·{" "}
                    {outcome.result.tokens_in} in / {outcome.result.tokens_out}{" "}
                    out tokens
                    {outcome.result.estimated ? " (estimated)" : ""}
                  </p>
                </div>
              )}
            </div>
          );
        })}
      </Section>

      <Section title="Add a custom OpenAI-compatible endpoint">
        <p className="text-xs text-neutral-500">
          Any server that speaks `POST /chat/completions` works: Ollama,
          llama.cpp, LM Studio, a gateway, or a vendor not listed above. Use a
          blank key for local servers.
        </p>
        <div className="grid gap-1.5 sm:grid-cols-2">
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Display name</span>
            <input
              value={customName}
              onChange={(event) => {
                const value = event.target.value;
                setCustomName(value);
                if (!customIdTouched) setCustomId(customIdFromName(value));
              }}
              placeholder="My local server"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Provider id</span>
            <input
              value={customId}
              onChange={(event) => {
                setCustomId(event.target.value);
                setCustomIdTouched(true);
              }}
              placeholder="custom.my-server"
              className="rounded-md border border-border bg-transparent px-2 py-1 font-mono text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs sm:col-span-2">
            <span className="text-neutral-500">Base URL</span>
            <input
              value={customUrl}
              onChange={(event) => setCustomUrl(event.target.value)}
              placeholder="http://localhost:11434/v1"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs sm:col-span-2">
            <span className="text-neutral-500">
              Models (comma or newline separated; blank is fine and can be
              synced later)
            </span>
            <input
              value={customModels}
              onChange={(event) => setCustomModels(event.target.value)}
              placeholder="llama3.1, qwen2.5-coder"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs sm:col-span-2">
            <span className="text-neutral-500">
              API key (blank for a local server; stored in the OS keychain)
            </span>
            <input
              type="password"
              value={customKey}
              autoComplete="off"
              onChange={(event) => setCustomKey(event.target.value)}
              placeholder="sk-…"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void addCustomProvider()}
            className="flex items-center gap-1 rounded-md border border-border px-2.5 py-1 text-xs"
          >
            <Plus size={12} /> Add endpoint
          </button>
          {customMessage && (
            <span
              className={`text-xs ${customMessage.includes("added") ? "text-green-700 dark:text-green-400" : "text-red-600"}`}
            >
              {customMessage}
            </span>
          )}
        </div>
        <p className="text-xs text-neutral-500">
          Save the configuration, then add the endpoint key in the provider list
          above.
        </p>
      </Section>

      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => void saveNow()}
          disabled={!dirty || save.isPending}
          className="rounded-md border border-border px-3 py-1.5 text-sm disabled:opacity-40"
        >
          {save.isPending ? "Saving…" : "Save model settings"}
        </button>
        {dirty && (
          <span className="text-xs text-amber-600">Unsaved changes</span>
        )}
        {message && (
          <span
            className={`text-xs ${message.includes("Saved") || message.includes("Synced") || message.includes("added") ? "text-green-700 dark:text-green-400" : "text-red-600"}`}
          >
            {message}
          </span>
        )}
      </div>
    </div>
  );
}
