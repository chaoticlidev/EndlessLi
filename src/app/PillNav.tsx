import {
  BookOpen,
  FlaskConical,
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
  { id: "research", label: "Research", shortcut: "2", icon: FlaskConical },
  { id: "library", label: "Library", shortcut: "3", icon: Library },
  { id: "skills", label: "Skills", shortcut: "4", icon: Sparkles },
];

/** Top pill navigation bar with keyboard shortcuts Ctrl/Cmd+1..4. */
export function PillNav() {
  const activeTab = useUi((state) => state.activeTab);
  const setActiveTab = useUi((state) => state.setActiveTab);

  return (
    <nav className="flex items-center gap-1 rounded-full border border-neutral-300 bg-white px-2 py-1 dark:border-neutral-700 dark:bg-neutral-900">
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
                ? "bg-neutral-900 text-white dark:bg-white dark:text-neutral-900"
                : "text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"
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
