import { useEffect } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { useUi } from "../stores/ui";
import { queryClient } from "../ipc/queries";
import { PillNav } from "./PillNav";
import { CommandPalette } from "./CommandPalette";
import { KnowledgeBaseView } from "./KnowledgeBaseView";
import { SettingsView } from "../features/settings/SettingsView";
import { ComingSoon } from "./ComingSoon";

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
          "skills",
          "settings",
        ] as const;
        setActiveTab(tabs[Number(event.key) - 1]);
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
      return (
        <ComingSoon
          title="Research"
          text="Search-based research with cited results arrives in the next milestone."
        />
      );
    case "library":
      return (
        <ComingSoon
          title="Library"
          text="Archived sources and corpus processing arrive in a later milestone."
        />
      );
    case "skills":
      return (
        <ComingSoon
          title="Skills"
          text="Reusable skills and scripts arrive in a later milestone."
        />
      );
    case "settings":
      return <SettingsView />;
  }
}

export function App() {
  useGlobalShortcuts();
  const paletteNonce = useUi((state) => state.paletteNonce);

  return (
    <QueryClientProvider client={queryClient}>
      <div className="flex h-screen flex-col gap-2 bg-neutral-100 p-3 text-neutral-900 dark:bg-neutral-950 dark:text-neutral-100">
        <header className="flex items-center justify-between gap-2">
          <span className="px-1 text-sm font-bold tracking-tight">
            EndlessLi
          </span>
          <PillNav />
          <span className="hidden w-24 text-right text-xs text-neutral-400 sm:block">
            Ctrl+K
          </span>
        </header>
        <main className="flex min-h-0 flex-1 flex-col">
          <ActiveTab />
        </main>
        <CommandPalette key={paletteNonce} />
      </div>
    </QueryClientProvider>
  );
}
