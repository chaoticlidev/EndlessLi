import { useEffect } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { Cog } from "lucide-react";
import { useUi } from "../stores/ui";
import { queryClient } from "../ipc/queries";
import { PillNav } from "./PillNav";
import { CommandPalette } from "./CommandPalette";
import { KnowledgeBaseView } from "./KnowledgeBaseView";
import { ResearchView } from "../features/research/ResearchView";
import { LibraryView } from "../features/library/LibraryView";
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
      if (mod && ["1", "2", "3", "4"].includes(event.key)) {
        event.preventDefault();
        const tabs = [
          "knowledge",
          "research",
          "library",
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
    case "skills":
      return <SkillsView />;
  }
}

export function App() {
  useGlobalShortcuts();
  const paletteNonce = useUi((state) => state.paletteNonce);
  const activeTab = useUi((state) => state.activeTab);
  const setActiveTab = useUi((state) => state.setActiveTab);
  const setSettingsOpen = useUi((state) => state.setSettingsOpen);

  useEffect(() => {
    setSettingsOpen(false);
  }, [activeTab, setSettingsOpen]);

  return (
    <QueryClientProvider client={queryClient}>
      <div className="flex h-screen flex-col gap-2 bg-neutral-100 p-3 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
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
              className="rounded p-1.5 text-neutral-500 hover:bg-neutral-100 hover:text-neutral-900 dark:hover:bg-neutral-800 dark:hover:text-neutral-100"
              title="Settings (Ctrl+,)"
            >
              <Cog size={18} />
            </button>
            <span className="hidden text-xs text-neutral-400 sm:inline-flex">
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
