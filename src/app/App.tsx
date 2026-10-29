import { useEffect } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { Cog } from "lucide-react";
import { useUi } from "../stores/ui";
import { useAppearanceTheme } from "../stores/appearance";
import { queryClient } from "../ipc/queries";
import { AppBackground } from "../components/AppBackground";
import { PillNav } from "./PillNav";
import { CommandPalette } from "./CommandPalette";
import { KnowledgeBaseView } from "./KnowledgeBaseView";
import { ResearchView } from "../features/research/ResearchView";
import { LibraryView } from "../features/library/LibraryView";
import { SynthesizeView } from "../features/synthesize/SynthesizeView";
import { SkillsView } from "../features/skills/SkillsView";
import { SettingsModal } from "../features/settings/SettingsView";
import endlessliLogo from '../../assets/endlessli-logo.png';

function useGlobalShortcuts() {
  const setPaletteOpen = useUi((state) => state.setPaletteOpen);
  const setActiveTab = useUi((state) => state.setActiveTab);

  useEffect(() => {
    function onKey(event: KeyboardEvent) {
      const mod = event.ctrlKey || event.metaKey;
      if (mod && event.key.toLowerCase() === "k") {
        event.preventDefault();
        useUi.getState().setPaletteOpen(!useUi.getState().paletteOpen);
        return;
      }
      if (mod && ["1", "2", "3", "4", "5"].includes(event.key)) {
        event.preventDefault();
        const tabs = [
          "knowledge",
          "research",
          "library",
          "synthesizes",
          "skills",
        ] as const;
        setActiveTab(tabs[Number(event.key) - 1]);
      }
      if (mod && event.key === ",") {
        event.preventDefault();
        useUi.getState().setSettingsOpen(true);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setPaletteOpen, setActiveTab]);
}

function ActiveTab() {
  const activeTab = useUi((state) => state.activeTab);
  switch (activeTab) {
    case "knowledge":
      return <KnowledgeBaseView />;
    case "research":
      return <ResearchView />;
    case "library":
      return <LibraryView />;
    case "synthesizes":
      return <SynthesizeView />;
    case "skills":
      return <SkillsView />;
  }
}

export function App() {
  useGlobalShortcuts();
  const resolvedTheme = useAppearanceTheme();
  const paletteNonce = useUi((state) => state.paletteNonce);
  const activeTab = useUi((state) => state.activeTab);
  const setActiveTab = useUi((state) => state.setActiveTab);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);

  useEffect(() => {
    setSettingsOpen(false);
  }, [activeTab, setSettingsOpen]);

  return (
    <QueryClientProvider client={queryClient}>
      {/* `isolate` keeps the background layer's negative z-index inside this
          shell; `bg-transparent` lets the base color come from AppBackground. */}
      <div className="relative isolate flex h-screen flex-col gap-2 p-3 text-foreground">
        <AppBackground tone={resolvedTheme} />
        <header className="flex items-center justify-between gap-2">
          <button onClick={() => setActiveTab("knowledge")}>
            <img src={endlessliLogo} alt="EndlessLi" className="h-8 w-8" />
          </button>
          <PillNav />
          <div className="flex items-center gap-0.5">
            <button
              type="button"
              aria-label="Open settings"
              onClick={() => setSettingsOpen(true)}
              className="rounded p-1.5 text-muted-foreground hover:bg-muted hover:text-foreground"
              title="Settings (Ctrl+,)"
            >
              <Cog size={18} />
            </button>
            <span className="hidden text-xs text-muted-foreground sm:inline-flex">
              Ctrl+K
            </span>
          </div>
        </header>
        <main className="flex min-h-0 flex-1 flex-col">
          <ActiveTab />
        </main>
        <CommandPalette key={paletteNonce} />
        <SettingsModal />
      </div>
    </QueryClientProvider>
  );
}
