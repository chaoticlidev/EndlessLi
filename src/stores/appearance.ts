import { useEffect, useState } from "react";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

/**
 * Appearance preferences for the whole window.
 *
 * These live in `localStorage` rather than the SQLite `settings` table on
 * purpose: the Rust command that owns settings only accepts the namespaces it
 * knows about (`src-tauri/src/commands/settings.rs`), and appearance is purely
 * a presentation concern that never needs to reach the backend. The webview's
 * localStorage survives application restarts, which is all the feature needs.
 */

export type ThemeMode = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";
export type BackgroundMode = "none" | "starfield" | "image";

export const APPEARANCE_STORAGE_KEY = "endlessli.appearance";

const DARK_QUERY = "(prefers-color-scheme: dark)";

export interface AppearanceState {
  /** What the user picked. `system` follows the OS preference live. */
  theme: ThemeMode;
  /** Which layer (if any) is painted behind the UI. */
  background: BackgroundMode;
  /** Downscaled data URL for the custom background, or null. */
  backgroundImage: string | null;
  /** Black scrim opacity (percent) used to keep text readable over an image. */
  backgroundDim: number;

  setTheme: (theme: ThemeMode) => void;
  setBackground: (background: BackgroundMode) => void;
  setBackgroundImage: (image: string | null) => void;
  setBackgroundDim: (dim: number) => void;
}

/** Resolves `system` against the live OS preference. */
export function resolveTheme(theme: ThemeMode): ResolvedTheme {
  if (theme !== "system") return theme;
  if (
    typeof window === "undefined" ||
    typeof window.matchMedia !== "function"
  ) {
    return "dark";
  }
  return window.matchMedia(DARK_QUERY).matches ? "dark" : "light";
}

/**
 * Writes the resolved theme onto <html> so the `dark:` variant and the
 * `--background` / `--card` token blocks switch together.
 */
export function applyThemeToDocument(theme: ThemeMode): ResolvedTheme {
  const resolved = resolveTheme(theme);
  if (typeof document !== "undefined") {
    document.documentElement.dataset.theme = resolved;
  }
  return resolved;
}

export const useAppearance = create<AppearanceState>()(
  persist(
    (set) => ({
      theme: "system",
      background: "none",
      backgroundImage: null,
      backgroundDim: 35,

      setTheme: (theme) => set({ theme }),
      setBackground: (background) => set({ background }),
      setBackgroundImage: (backgroundImage) => set({ backgroundImage }),
      setBackgroundDim: (backgroundDim) =>
        set({ backgroundDim: clampDim(backgroundDim) }),
    }),
    {
      name: APPEARANCE_STORAGE_KEY,
      version: 1,
      // Only the values are persisted; the setters are recreated on every load.
      partialize: (state) => ({
        theme: state.theme,
        background: state.background,
        backgroundImage: state.backgroundImage,
        backgroundDim: state.backgroundDim,
      }),
      storage: createJSONStorage(() => localStorage),
    },
  ),
);

function clampDim(value: number): number {
  if (!Number.isFinite(value)) return 35;
  return Math.min(80, Math.max(0, Math.round(value)));
}

// localStorage rehydrates synchronously, so this runs before the first render
// and the window never flashes the wrong palette.
applyThemeToDocument(useAppearance.getState().theme);

/**
 * Keeps `data-theme` in sync and returns the effective theme.
 *
 * Call this once at the top of the app. All other components read
 * `useAppearance` directly; only the root needs to own the DOM side effect.
 */
export function useAppearanceTheme(): ResolvedTheme {
  const theme = useAppearance((state) => state.theme);
  const [resolved, setResolved] = useState<ResolvedTheme>(() =>
    resolveTheme(theme),
  );

  useEffect(() => {
    const media =
      typeof window.matchMedia === "function"
        ? window.matchMedia(DARK_QUERY)
        : null;

    function sync() {
      setResolved(applyThemeToDocument(theme));
    }

    sync();
    if (theme !== "system" || !media) return undefined;
    media.addEventListener("change", sync);
    return () => media.removeEventListener("change", sync);
  }, [theme]);

  return resolved;
}
