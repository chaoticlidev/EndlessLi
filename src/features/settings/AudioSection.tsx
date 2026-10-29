import { useState } from "react";
import { Loader2, Play, Plus, Trash2, Volume2 } from "lucide-react";
import type { TtsConfig, TtsCustomProvider, TtsSample } from "../../ipc/bindings";
import { commands } from "../../ipc/bindings";
import {
  errorMessage,
  useSaveTtsConfig,
  useSetKey,
  useTtsConfig,
  useTtsVoices,
} from "../../ipc/queries";
import { Select } from "../../components/Select";
import { ProviderKeyRow, Section } from "./primitives";

const ENGINES = [
  { value: "kokoro", label: "Kokoro (Local)" },
  { value: "piper", label: "Piper (Local)" },
  { value: "openai", label: "Custom OpenAI Speech Endpoint" },
];

function customIdFromName(name: string): string {
  const slug = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .replace(/-{2,}/g, "-")
    .slice(0, 40)
    .replace(/-+$/g, "");
  return `tts.${slug || "endpoint"}`;
}

function splitList(value: string): string[] {
  return value
    .split(/[\n,]/)
    .map((entry) => entry.trim())
    .filter(Boolean);
}

/**
 * Audio settings: engine, voice, local model paths, and custom speech
 * endpoints, with a sample button that synthesizes before saving.
 *
 * Local engines keep audio on the machine. Custom endpoints store their key in
 * the OS keychain through the same write-only path as LLM keys.
 */
export function AudioSection() {
  const { data: stored, isLoading } = useTtsConfig();
  const save = useSaveTtsConfig();
  const setKey = useSetKey();
  const [draft, setDraft] = useState<TtsConfig | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [playing, setPlaying] = useState(false);
  const [sample, setSample] = useState<TtsSample | null>(null);

  const [customName, setCustomName] = useState("");
  const [customId, setCustomId] = useState("");
  const [customIdTouched, setCustomIdTouched] = useState(false);
  const [customUrl, setCustomUrl] = useState("");
  const [customModels, setCustomModels] = useState("tts-1");
  const [customVoices, setCustomVoices] = useState("alloy, nova");
  const [customKey, setCustomKey] = useState("");
  const [customMessage, setCustomMessage] = useState<string | null>(null);

  const config = draft ?? stored;
  const engine = config?.engine ?? "kokoro";
  const providerId = config?.custom_provider_id ?? "";
  const {
    data: voices = [],
    isFetching: loadingVoices,
    error: voicesError,
  } = useTtsVoices(
    engine,
    providerId,
    config?.piper_voices_dir ?? "",
    config?.kokoro_voices_path ?? "",
  );

  if (isLoading || !config) {
    return (
      <Section>
        <p className="flex items-center gap-2 text-sm text-neutral-500">
          <Loader2 size={14} className="animate-spin" /> Loading audio settings…
        </p>
      </Section>
    );
  }
  const cfg: TtsConfig = config;

  const activeVoice =
    engine === "openai" ? cfg.custom_voice : cfg.voice;
  const voiceOptions = voices.map((voice) => ({
    value: voice.id,
    label: voice.label,
  }));
  if (activeVoice && !voiceOptions.some((entry) => entry.value === activeVoice)) {
    voiceOptions.unshift({ value: activeVoice, label: activeVoice });
  }
  const selectedProvider = cfg.custom_providers.find(
    (custom) => custom.id === cfg.custom_provider_id,
  );

  function update(change: (current: TtsConfig) => TtsConfig) {
    setDraft((current) => change(current ?? cfg));
    setMessage(null);
  }

  async function saveNow() {
    setMessage(null);
    try {
      await save.mutateAsync(cfg);
      setDraft(null);
      setMessage("Saved and active");
    } catch (err) {
      setMessage(errorMessage(err));
    }
  }

  async function playSample() {
    if (!activeVoice) {
      setMessage("Choose a voice first.");
      return;
    }
    setPlaying(true);
    setMessage(null);
    setSample(null);
    try {
      const result = await commands.ttsSynthesize({
        engine: cfg.engine,
        voice: activeVoice,
        text: null,
        custom_provider_id: cfg.custom_provider_id || null,
        custom_model: cfg.custom_model || null,
        custom_base_url: null,
        kokoro_model_path: cfg.kokoro_model_path || null,
        kokoro_voices_path: cfg.kokoro_voices_path || null,
        piper_path: cfg.piper_path || null,
        piper_voices_dir: cfg.piper_voices_dir || null,
      });
      if (result.status === "ok") {
        setSample(result.data);
        try {
          const audio = new Audio(result.data.data_url);
          await audio.play();
        } catch {
          // Autoplay can be blocked; the inline player below still works.
        }
      } else {
        setMessage(result.error.message);
      }
    } catch (err) {
      setMessage(errorMessage(err));
    } finally {
      setPlaying(false);
    }
  }

  async function addCustomProvider() {
    const name = customName.trim();
    if (!name) {
      setCustomMessage("Give the endpoint a display name.");
      return;
    }
    const id = (customId.trim() || customIdFromName(name)).toLowerCase();
    if (!/^tts\.[a-z0-9]+(?:-[a-z0-9]+)*$/.test(id)) {
      setCustomMessage("Endpoint id must look like tts.my-endpoint.");
      return;
    }
    if (!/^https?:\/\//.test(customUrl.trim())) {
      setCustomMessage("Base URL must start with http:// or https://.");
      return;
    }
    if (cfg.custom_providers.some((custom) => custom.id === id)) {
      setCustomMessage("That endpoint id is already used.");
      return;
    }
    if (customKey.trim()) {
      try {
        await setKey.mutateAsync({ provider: id, key: customKey.trim() });
      } catch (err) {
        setCustomMessage(`Could not store the key: ${errorMessage(err)}`);
        return;
      }
    }
    const provider: TtsCustomProvider = {
      id,
      display_name: name,
      base_url: customUrl.trim().replace(/\/+$/, ""),
      models: splitList(customModels),
      voices: splitList(customVoices),
    };
    update((current) => ({
      ...current,
      custom_providers: [...current.custom_providers, provider],
      custom_provider_id: id,
      custom_model: provider.models[0] ?? current.custom_model,
      custom_voice: provider.voices[0] ?? current.custom_voice,
    }));
    setCustomName("");
    setCustomId("");
    setCustomIdTouched(false);
    setCustomUrl("");
    setCustomKey("");
    setCustomMessage("Endpoint added. Save to activate it.");
  }

  function removeCustomProvider(id: string) {
    update((current) => ({
      ...current,
      custom_providers: current.custom_providers.filter(
        (custom) => custom.id !== id,
      ),
      custom_provider_id:
        current.custom_provider_id === id ? "" : current.custom_provider_id,
    }));
  }

  const dirty = draft !== null;

  return (
    <div className="flex flex-col gap-3">
      <Section title="Default engine">
        <label className="flex flex-col gap-1 text-sm">
          <span className="font-medium">Engine</span>
          <Select
            ariaLabel="TTS engine"
            value={cfg.engine}
            onChange={(value) =>
              update((current) => ({ ...current, engine: value, voice: "" }))
            }
            options={ENGINES}
          />
          <span className="text-xs text-neutral-500">
            Kokoro and Piper run locally. The custom endpoint sends text to any
            server that implements `POST /audio/speech`.
          </span>
        </label>

        <label className="flex flex-col gap-1 text-sm">
          <span className="font-medium">Voice</span>
          <span className="flex gap-1.5">
            <Select
              ariaLabel="Voice"
              value={activeVoice}
              onChange={(value) =>
                update((current) =>
                  current.engine === "openai"
                    ? { ...current, custom_voice: value }
                    : { ...current, voice: value },
                )
              }
              options={voiceOptions}
              placeholder={
                loadingVoices ? "Loading voices…" : "Choose a voice"
              }
            />
            <button
              type="button"
              onClick={() => void playSample()}
              disabled={playing || !activeVoice}
              className="flex shrink-0 items-center gap-1 rounded-md border border-border px-2.5 py-1 text-xs disabled:opacity-40"
            >
              {playing ? (
                <Loader2 size={12} className="animate-spin" />
              ) : (
                <Play size={12} />
              )}
              Play sample
            </button>
          </span>
          <span className="text-xs text-neutral-500">
            Voices are listed from the configured model files or endpoint. The
            sample uses the engine and paths currently shown, even before you
            save.
          </span>
          {voicesError && (
            <span className="text-xs text-amber-600">
              Could not list voices: {errorMessage(voicesError)}
            </span>
          )}
        </label>

        {sample && (
          <div className="flex flex-col gap-1 rounded-md bg-neutral-100 p-2 text-xs dark:bg-neutral-800">
            <span className="flex items-center gap-1">
              <Volume2 size={12} /> {sample.engine} · {sample.voice} ·{" "}
              {Math.round(sample.bytes / 1024)} KiB
            </span>
            <audio controls src={sample.data_url} className="w-full" />
          </div>
        )}
        {message && (
          <p
            className={`text-xs ${message.includes("Saved") ? "text-green-700 dark:text-green-400" : "text-red-600"}`}
          >
            {message}
          </p>
        )}
      </Section>

      {engine === "kokoro" && (
        <Section title="Kokoro (local)">
          <p className="text-xs text-neutral-500">
            Requires the optional `kokoro-onnx` package in the sidecar
            environment, a downloaded `.onnx` model, and the matching `.bin`
            voices archive. Audio never leaves the machine.
          </p>
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Model file (.onnx)</span>
            <input
              value={cfg.kokoro_model_path}
              onChange={(event) =>
                update((current) => ({
                  ...current,
                  kokoro_model_path: event.target.value,
                }))
              }
              placeholder="C:\models\kokoro-v1.0.onnx"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Voices file (.bin)</span>
            <input
              value={cfg.kokoro_voices_path}
              onChange={(event) =>
                update((current) => ({
                  ...current,
                  kokoro_voices_path: event.target.value,
                }))
              }
              placeholder="C:\models\voices-v1.0.bin"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
            {cfg.kokoro_voices_path.trim().toLowerCase().endsWith(".json") && (
              <span className="text-red-600">
                kokoro-onnx only reads the `.bin` voices archive. Download
                voices-v1.0.bin from the kokoro-onnx model releases.
              </span>
            )}
          </label>
        </Section>
      )}

      {engine === "piper" && (
        <Section title="Piper (local)">
          <p className="text-xs text-neutral-500">
            Point at the Piper executable and a directory of `.onnx` voice
            models. Each model appears in the voice list.
          </p>
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Piper executable</span>
            <input
              value={cfg.piper_path}
              onChange={(event) =>
                update((current) => ({
                  ...current,
                  piper_path: event.target.value,
                }))
              }
              placeholder="C:\tools\piper\piper.exe"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Voices directory</span>
            <input
              value={cfg.piper_voices_dir}
              onChange={(event) =>
                update((current) => ({
                  ...current,
                  piper_voices_dir: event.target.value,
                }))
              }
              placeholder="C:\tools\piper\voices"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
        </Section>
      )}

      {engine === "openai" && (
        <Section title="Custom OpenAI Speech endpoint">
          <label className="flex flex-col gap-1 text-sm">
            <span className="font-medium">Endpoint</span>
            <Select
              ariaLabel="Speech endpoint"
              value={cfg.custom_provider_id}
              onChange={(value) => {
                const provider = cfg.custom_providers.find(
                  (custom) => custom.id === value,
                );
                update((current) => ({
                  ...current,
                  custom_provider_id: value,
                  custom_model: provider?.models[0] ?? current.custom_model,
                  custom_voice: provider?.voices[0] ?? current.custom_voice,
                }));
              }}
              options={[
                { value: "", label: "Choose an endpoint…" },
                ...cfg.custom_providers.map((custom) => ({
                  value: custom.id,
                  label: custom.display_name,
                })),
              ]}
            />
          </label>
          {selectedProvider && (
            <>
              <ProviderKeyRow
                provider={selectedProvider.id}
                label={selectedProvider.display_name}
              />
              <div className="grid gap-1.5 sm:grid-cols-2">
                <label className="flex flex-col gap-0.5 text-xs">
                  <span className="text-neutral-500">Model</span>
                  <input
                    list="tts-models"
                    value={cfg.custom_model}
                    onChange={(event) =>
                      update((current) => ({
                        ...current,
                        custom_model: event.target.value,
                      }))
                    }
                    placeholder={selectedProvider.models[0] ?? "tts-1"}
                    className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
                  />
                  <datalist id="tts-models">
                    {selectedProvider.models.map((model) => (
                      <option key={model} value={model} />
                    ))}
                  </datalist>
                </label>
                <label className="flex flex-col gap-0.5 text-xs">
                  <span className="text-neutral-500">Base URL</span>
                  <input
                    value={cfg.custom_provider_id
                      ? (cfg.custom_providers.find(
                          (custom) => custom.id === cfg.custom_provider_id,
                        )?.base_url ?? "")
                      : ""}
                    readOnly
                    className="rounded-md border border-border bg-transparent px-2 py-1 text-sm text-neutral-500 outline-none"
                  />
                </label>
              </div>
            </>
          )}
          {!selectedProvider && (
            <p className="text-xs text-amber-600">
              Add an endpoint below, save it, then choose it here.
            </p>
          )}
        </Section>
      )}

      <Section title="Speech endpoints">
        {cfg.custom_providers.map((custom) => (
          <div
            key={custom.id}
            className="flex flex-col gap-1 rounded-md border border-neutral-200 p-2.5 text-xs dark:border-neutral-700"
          >
            <div className="flex items-center justify-between gap-2">
              <span className="font-medium">{custom.display_name}</span>
              <button
                type="button"
                onClick={() => removeCustomProvider(custom.id)}
                className="flex items-center gap-1 text-red-500"
              >
                <Trash2 size={11} /> Remove
              </button>
            </div>
            <span className="font-mono text-[11px] text-neutral-500">
              {custom.id} · {custom.base_url}
            </span>
            <span className="text-neutral-500">
              Models: {custom.models.join(", ") || "none"} · Voices:{" "}
              {custom.voices.join(", ") || "default"}
            </span>
            <ProviderKeyRow provider={custom.id} label={custom.display_name} />
          </div>
        ))}

        <div className="grid gap-1.5 border-t border-border pt-2 sm:grid-cols-2">
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Display name</span>
            <input
              value={customName}
              onChange={(event) => {
                const value = event.target.value;
                setCustomName(value);
                if (!customIdTouched) setCustomId(customIdFromName(value));
              }}
              placeholder="My speech server"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Endpoint id</span>
            <input
              value={customId}
              onChange={(event) => {
                setCustomId(event.target.value);
                setCustomIdTouched(true);
              }}
              placeholder="tts.my-server"
              className="rounded-md border border-border bg-transparent px-2 py-1 font-mono text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs sm:col-span-2">
            <span className="text-neutral-500">Base URL</span>
            <input
              value={customUrl}
              onChange={(event) => setCustomUrl(event.target.value)}
              placeholder="https://api.openai.com/v1"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Models (comma separated)</span>
            <input
              value={customModels}
              onChange={(event) => setCustomModels(event.target.value)}
              placeholder="tts-1, gpt-4o-mini-tts"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs">
            <span className="text-neutral-500">Voices (comma separated)</span>
            <input
              value={customVoices}
              onChange={(event) => setCustomVoices(event.target.value)}
              placeholder="alloy, nova"
              className="rounded-md border border-border bg-transparent px-2 py-1 text-sm outline-none"
            />
          </label>
          <label className="flex flex-col gap-0.5 text-xs sm:col-span-2">
            <span className="text-neutral-500">
              API key (stored in the OS keychain)
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
      </Section>

      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={() => void saveNow()}
          disabled={!dirty || save.isPending}
          className="rounded-md border border-border px-3 py-1.5 text-sm disabled:opacity-40"
        >
          {save.isPending ? "Saving…" : "Save audio settings"}
        </button>
        {dirty && (
          <span className="text-xs text-amber-600">Unsaved changes</span>
        )}
      </div>
    </div>
  );
}
