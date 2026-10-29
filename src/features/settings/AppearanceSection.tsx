import { useRef, useState } from "react";
import type { ChangeEvent } from "react";
import {
  CircleOff,
  Image as ImageIcon,
  Monitor,
  Moon,
  Sparkles,
  Sun,
  Trash2,
  Upload,
} from "lucide-react";
import { Section } from "./primitives";
import { useAppearance } from "../../stores/appearance";
import type { BackgroundMode, ThemeMode } from "../../stores/appearance";

/**
 * Appearance settings: window theme plus the layered background.
 *
 * All values are stored by `src/stores/appearance.ts` in localStorage, so they
 * survive a restart without passing through the settings table in SQLite.
 * Images are downscaled before storage because localStorage has a small,
 * shared quota.
 */

const MAX_EDGE = 1920;
/** ~3 MB of base64 plus the rest of the state stays under the usual 5 MB. */
const MAX_DATA_URL_LENGTH = 3_000_000;
const IMAGE_QUALITY = 0.85;

const THEME_OPTIONS: {
  value: ThemeMode;
  label: string;
  icon: typeof Sun;
  hint: string;
}[] = [
  { value: "light", label: "Light", icon: Sun, hint: "Off-white surfaces" },
  { value: "dark", label: "Dark", icon: Moon, hint: "Pitch black surfaces" },
  {
    value: "system",
    label: "System",
    icon: Monitor,
    hint: "Follow the OS preference",
  },
];

const BACKGROUND_OPTIONS: {
  value: BackgroundMode;
  label: string;
  icon: typeof Sun;
  hint: string;
}[] = [
  {
    value: "none",
    label: "Plain",
    icon: CircleOff,
    hint: "Just the base color",
  },
  {
    value: "starfield",
    label: "Starfield",
    icon: Sparkles,
    hint: "Animated drifting stars",
  },
  {
    value: "image",
    label: "Image",
    icon: ImageIcon,
    hint: "Use your own picture",
  },
];

function loadImage(source: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () =>
      reject(new Error("That file could not be read as an image."));
    image.src = source;
  });
}

/**
 * Converts a picked file into a data URL the webview may render.
 *
 * The Tauri asset protocol is not enabled for this app and the CSP only allows
 * `data:` and `https:` images, so the file is decoded locally and re-encoded at
 * a bounded size instead of storing a path the webview could not load.
 */
async function fileToBackgroundDataUrl(file: File): Promise<string> {
  if (!file.type.startsWith("image/")) {
    throw new Error("Choose an image file (PNG, JPEG, WebP, or GIF).");
  }

  const objectUrl = URL.createObjectURL(file);
  try {
    const image = await loadImage(objectUrl);
    const sourceWidth = image.naturalWidth || image.width;
    const sourceHeight = image.naturalHeight || image.height;
    const scale = Math.min(1, MAX_EDGE / Math.max(sourceWidth, sourceHeight));
    const width = Math.max(1, Math.round(sourceWidth * scale));
    const height = Math.max(1, Math.round(sourceHeight * scale));

    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    const context = canvas.getContext("2d");
    if (!context) throw new Error("This webview cannot render a background.");
    context.drawImage(image, 0, 0, width, height);

    const dataUrl = canvas.toDataURL("image/webp", IMAGE_QUALITY);
    if (dataUrl.length > MAX_DATA_URL_LENGTH) {
      throw new Error(
        "That image is too large even after resizing. Try a smaller file.",
      );
    }
    return dataUrl;
  } finally {
    URL.revokeObjectURL(objectUrl);
  }
}

/** Only https and data URLs can render under the app's CSP. */
function normalizeImageUrl(raw: string): string | null {
  const value = raw.trim();
  if (!/^https:\/\/[^\s"'()]+$/i.test(value)) return null;
  return value;
}

function optionClasses(active: boolean): string {
  return `flex flex-col gap-0.5 rounded-md border px-3 py-2 text-left text-xs transition-colors ${
    active
      ? "border-foreground bg-muted text-foreground"
      : "border-border text-muted-foreground hover:bg-muted hover:text-foreground"
  }`;
}

export function AppearanceSection() {
  const theme = useAppearance((state) => state.theme);
  const background = useAppearance((state) => state.background);
  const backgroundImage = useAppearance((state) => state.backgroundImage);
  const backgroundDim = useAppearance((state) => state.backgroundDim);

  const setTheme = useAppearance((state) => state.setTheme);
  const setBackground = useAppearance((state) => state.setBackground);
  const setBackgroundImage = useAppearance((state) => state.setBackgroundImage);
  const setBackgroundDim = useAppearance((state) => state.setBackgroundDim);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const [urlDraft, setUrlDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  function clearFeedback() {
    setMessage(null);
    setError(null);
  }

  function storeImage(dataUrl: string | null) {
    try {
      setBackgroundImage(dataUrl);
      return true;
    } catch {
      // localStorage rejected the write (quota). Keep the previous value so the
      // app and the persisted state never disagree.
      setBackgroundImage(null);
      setError(
        "This image is too large to save. Try a smaller picture or paste a URL instead.",
      );
      return false;
    }
  }

  async function onPickFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    // Reset so picking the same file twice still fires a change event.
    event.target.value = "";
    if (!file) return;

    clearFeedback();
    setBusy(true);
    try {
      const dataUrl = await fileToBackgroundDataUrl(file);
      if (storeImage(dataUrl)) {
        setBackground("image");
        setMessage("Background image saved.");
      }
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "That image could not be used.",
      );
    } finally {
      setBusy(false);
    }
  }

  function onApplyUrl() {
    clearFeedback();
    const normalized = normalizeImageUrl(urlDraft);
    if (!normalized) {
      setError("Enter a full https:// image URL.");
      return;
    }
    if (storeImage(normalized)) {
      setBackground("image");
      setUrlDraft("");
      setMessage("Background image saved.");
    }
  }

  function onRemoveImage() {
    clearFeedback();
    if (storeImage(null)) {
      setBackground("none");
      setMessage("Background image removed.");
    }
  }

  return (
    <div className="flex flex-col gap-3">
      <Section title="Theme">
        <p className="text-xs text-muted-foreground">
          Applies to the whole window. System follows your OS and updates when
          it changes.
        </p>
        <div className="grid grid-cols-3 gap-2">
          {THEME_OPTIONS.map((option) => {
            const Icon = option.icon;
            const active = theme === option.value;
            return (
              <button
                key={option.value}
                type="button"
                aria-pressed={active}
                onClick={() => setTheme(option.value)}
                className={optionClasses(active)}
              >
                <span className="flex items-center gap-1.5 text-sm font-medium">
                  <Icon size={14} />
                  {option.label}
                </span>
                <span className="text-[11px] text-muted-foreground">
                  {option.hint}
                </span>
              </button>
            );
          })}
        </div>
      </Section>

      <Section title="Background">
        <p className="text-xs text-muted-foreground">
          Sits behind every panel. Panels stay translucent so the background
          remains visible without hurting readability.
        </p>
        <div className="grid grid-cols-3 gap-2">
          {BACKGROUND_OPTIONS.map((option) => {
            const Icon = option.icon;
            const active = background === option.value;
            return (
              <button
                key={option.value}
                type="button"
                aria-pressed={active}
                onClick={() => setBackground(option.value)}
                className={optionClasses(active)}
              >
                <span className="flex items-center gap-1.5 text-sm font-medium">
                  <Icon size={14} />
                  {option.label}
                </span>
                <span className="text-[11px] text-muted-foreground">
                  {option.hint}
                </span>
              </button>
            );
          })}
        </div>

        {background === "image" && !backgroundImage && (
          <p className="text-xs text-muted-foreground">
            No image selected yet. Upload one or paste an https URL below.
          </p>
        )}

        {backgroundImage && (
          <div
            role="img"
            aria-label="Current background preview"
            className="h-24 w-full rounded-md border border-border bg-cover bg-center bg-no-repeat"
            style={{ backgroundImage: `url("${backgroundImage}")` }}
          />
        )}

        {background === "image" && (
          <label className="flex flex-col gap-1 text-xs">
            <span className="flex items-center justify-between font-medium">
              <span>Darkening overlay</span>
              <span className="text-muted-foreground">{backgroundDim}%</span>
            </span>
            <input
              type="range"
              min={0}
              max={80}
              step={5}
              value={backgroundDim}
              onChange={(event) => setBackgroundDim(Number(event.target.value))}
              className="w-full"
            />
            <span className="text-muted-foreground">
              Increase this if text is hard to read over a bright image.
            </span>
          </label>
        )}
      </Section>

      <Section title="Custom image">
        <p className="text-xs text-muted-foreground">
          Images are resized to 1920px on the long edge and stored locally on
          this machine.
        </p>

        <div className="flex flex-wrap items-center gap-2">
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(event) => void onPickFile(event)}
          />
          <button
            type="button"
            disabled={busy}
            onClick={() => fileInputRef.current?.click()}
            className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm disabled:opacity-40"
          >
            <Upload size={14} />
            {busy ? "Processing…" : "Upload image"}
          </button>
          {backgroundImage && (
            <button
              type="button"
              onClick={onRemoveImage}
              className="flex items-center gap-1.5 rounded-md border border-border px-3 py-1.5 text-sm text-muted-foreground hover:text-foreground"
            >
              <Trash2 size={14} />
              Remove
            </button>
          )}
        </div>

        <div className="flex flex-col gap-1 text-xs">
          <span className="font-medium">Or paste an image URL</span>
          <span className="flex gap-1.5">
            <input
              value={urlDraft}
              onChange={(event) => setUrlDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") onApplyUrl();
              }}
              placeholder="https://…"
              className="w-full rounded-md border border-border bg-transparent px-2 py-1 text-xs outline-none"
            />
            <button
              type="button"
              onClick={onApplyUrl}
              disabled={!urlDraft.trim()}
              className="shrink-0 rounded-md border border-border px-2 py-1 text-xs disabled:opacity-40"
            >
              Apply
            </button>
          </span>
          <span className="text-muted-foreground">
            Only https URLs load. Local files must be uploaded above.
          </span>
        </div>

        {message && (
          <span className="text-xs text-green-700 dark:text-green-400">
            {message}
          </span>
        )}
        {error && (
          <span className="text-xs text-red-600 dark:text-red-400">
            {error}
          </span>
        )}
      </Section>

      <p className="text-[11px] text-muted-foreground">
        Starfield motion is disabled automatically when your OS requests reduced
        motion. Settings here are remembered between launches.
      </p>
    </div>
  );
}
