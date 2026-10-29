import {
  BookOpen,
  FlaskConical,
  Globe,
  Library,
  Sparkles,
} from "lucide-react";
import { useUi } from "../stores/ui";
import type { TabId } from "../stores/ui";

const TABS: {
  id: TabId;
  label: string;
  shortcut: string;
  icon: typeof BookOpen;
}[] = [
  { id: "knowledge", label: "Knowledge Base", shortcut: "1", icon: BookOpen },
  { id: "research", label: "Research", shortcut: "2", icon: Globe },
  { id: "library", label: "Library", shortcut: "3", icon: Library },
  { id: "synthesizes", label: "Synthesize", shortcut: "4", icon: FlaskConical },
  { id: "skills", label: "Skills", shortcut: "5", icon: Sparkles },
];

/** Top pill navigation bar with keyboard shortcuts Ctrl/Cmd+1..4. */
export function PillNav() {
  const activeTab = useUi((state) => state.activeTab);
  const setActiveTab = useUi((state) => state.setActiveTab);

  return (
    <nav className="flex items-center gap-1 rounded-full border border-border bg-panel px-2 py-1 shadow-sm">
      {TABS.map((tab) => {
        const Icon = tab.icon;
        const active = tab.id === activeTab;
        return (
          <button
            key={tab.id}
            type="button"
            title={`${tab.label} (Ctrl+${tab.shortcut})`}
            onClick={() => setActiveTab(tab.id)}
            className={`flex items-center gap-1.5 rounded-full px-3 py-1.5 text-sm transition-colors ${
              active
                ? "bg-foreground font-medium text-background shadow-sm"
                : "text-muted-foreground hover:bg-muted hover:text-foreground"
            }`}
          >
            <Icon size={15} strokeWidth={2} />
            <span>{tab.label}</span>
          </button>
        );
      })}
    </nav>
  );
}