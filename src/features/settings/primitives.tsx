import { useState } from "react";
import type { ReactNode } from "react";
import { Check, KeyRound, Loader2, Trash2 } from "lucide-react";
import {
  errorMessage,
  useDeleteKey,
  useKeyStatus,
  useSetKey,
  useSetSetting,
  useSetting,
} from "../../ipc/queries";

/** Card wrapper shared by every Settings tab. */
export function Section({
  title,
  children,
}: {
  title?: string;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2 rounded-lg border border-border bg-muted/30 p-4">
      {title && <h2 className="text-sm font-semibold">{title}</h2>}
      {children}
    </section>
  );
}

/**
 * API-key editor for one provider.
 *
 * Keys are write-only: saving and deleting go straight to the OS keychain, and
 * the stored value is never read back into the input.
 */
export function ProviderKeyRow({
  provider,
  label,
}: {
  provider: string;
  label: string;
}) {
  const { data: status } = useKeyStatus();
  const setKey = useSetKey();
  const deleteKey = useDeleteKey();
  const [draft, setDraft] = useState("");
  const [message, setMessage] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const configured =
    status?.find((entry) => entry.provider === provider)?.configured ?? false;

  async function save() {
    const key = draft.trim();
    if (!key) return;
    setBusy(true);
    setMessage(null);
    try {
      await setKey.mutateAsync({ provider, key });
      setDraft("");
      setMessage("Key stored");
    } catch (err) {
      setMessage(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    setBusy(true);
    setMessage(null);
    try {
      await deleteKey.mutateAsync(provider);
      setMessage("Key deleted");
    } catch (err) {
      setMessage(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex flex-col gap-1">
      <span className="flex items-center gap-2 text-xs">
        <KeyRound size={13} className="text-neutral-400" />
        <span className="font-medium">{label} key</span>
        <span
          className={`flex items-center gap-1 ${configured ? "text-green-700 dark:text-green-400" : "text-neutral-400"}`}
        >
          {configured && <Check size={11} />}
          {configured ? "Stored" : "Not set"}
        </span>
      </span>
      <span className="flex gap-1.5">
        <input
          type="password"
          value={draft}
          placeholder={configured ? "Replace key…" : "Paste API key…"}
          autoComplete="off"
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") void save();
          }}
          className="w-full rounded-md border border-border bg-transparent px-2 py-1 text-xs outline-none"
        />
        <button
          type="button"
          onClick={() => void save()}
          disabled={busy || !draft.trim()}
          className="shrink-0 rounded-md border border-border px-2 py-1 text-xs disabled:opacity-40"
        >
          {busy ? <Loader2 size={12} className="animate-spin" /> : "Save"}
        </button>
        {configured && (
          <button
            type="button"
            title={`Delete ${label} key`}
            onClick={() => void remove()}
            disabled={busy}
            className="shrink-0 rounded-md border border-border px-2 py-1 text-xs disabled:opacity-40"
          >
            <Trash2 size={12} />
          </button>
        )}
      </span>
      {message && (
        <span
          className={`text-xs ${message.includes("stored") || message.includes("deleted") ? "text-green-700 dark:text-green-400" : "text-red-600"}`}
        >
          {message}
        </span>
      )}
    </div>
  );
}

/** A single key/value setting with an explicit Save action. */
export function SettingField({
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
          className="w-full rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
        />
        <button
          type="button"
          onClick={() => void save()}
          disabled={draft === null}
          className="shrink-0 rounded-md border border-border px-2.5 py-1 text-xs disabled:opacity-40"
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
