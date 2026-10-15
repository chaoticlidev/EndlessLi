import { useState } from "react";
import type { ReactNode } from "react";
import { Check, KeyRound, Loader2, Trash2, FlaskConical } from "lucide-react";
import { commands } from "../../ipc/bindings";
import type { TestOutcome } from "../../ipc/bindings";
import {
  errorMessage,
  useDeleteKey,
  useKeyStatus,
  useSetKey,
  useSetSetting,
  useSetting,
  useTokenSummary,
} from "../../ipc/queries";

const LLM_PROVIDERS = [
  { id: "openai", label: "OpenAI", defaultModel: "gpt-4o-mini" },
  { id: "anthropic", label: "Anthropic", defaultModel: "claude-sonnet-4-5" },
  { id: "deepseek", label: "DeepSeek", defaultModel: "deepseek-chat" },
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
  const { data: status } = useKeyStatus();
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
                    onClick={() => deleteKey.mutate(provider.id)}
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

/** Settings tab: providers, keys, models, embedding, cache, budgets. */
export function SettingsView() {
  const { data: provider } = useSetting("llm.provider");
  const setSetting = useSetSetting();
  const current = provider || "openai";

  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col gap-3 overflow-y-auto pb-4">
      <Section title="Language model">
        <label className="flex flex-col gap-1 text-sm">
          <span className="font-medium">Provider</span>
          <select
            value={current}
            onChange={(event) => {
              void setSetting.mutateAsync({
                key: "llm.provider",
                value: event.target.value,
              });
            }}
            className="rounded-md border border-neutral-300 bg-transparent px-2 py-1 text-sm dark:border-neutral-600"
          >
            {LLM_PROVIDERS.map((entry) => (
              <option key={entry.id} value={entry.id}>
                {entry.label}
              </option>
            ))}
          </select>
        </label>
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

      <Section title="API keys">
        <ProviderKeys />
      </Section>

      <Section title="Embedding">
        <SettingField
          settingKey="embedding.model"
          label="Embedding model"
          hint="Recorded now, used for auto-linking from the next milestone onward."
          placeholder="all-MiniLM-L6-v2"
        />
      </Section>

      <Section title="Cache">
        <SettingField
          settingKey="cache.max_bytes"
          label="Maximum cache size (bytes)"
          hint="Least-recently-used entries are evicted past this size. Default 1073741824 (1 GB)."
          placeholder="1073741824"
        />
      </Section>

      <Section title="Token budgets">
        <Budgets />
      </Section>
    </div>
  );
}
