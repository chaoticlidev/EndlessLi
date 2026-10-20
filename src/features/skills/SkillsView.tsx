import { useState } from "react";
import { Sparkles, Terminal } from "lucide-react";
import { ScriptWorkspace } from "./ScriptWorkspace";
import { SkillWorkspace } from "./SkillWorkspace";

type SubTab = "skills" | "scripts";

function SubTabButton({
  active,
  icon: Icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: typeof Sparkles;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex items-center gap-1.5 px-3 py-1.5 text-sm ${
        active
          ? "bg-neutral-900 text-white dark:bg-white dark:text-neutral-900"
          : "text-neutral-600 hover:bg-neutral-100 dark:text-neutral-300 dark:hover:bg-neutral-800"
      }`}
    >
      <Icon size={14} />
      {label}
    </button>
  );
}

/** Skills tab: a Skills/Scripts sub-tab switch over the two workspaces. */
export function SkillsView() {
  const [subTab, setSubTab] = useState<SubTab>("skills");

  return (
    <div className="flex h-full min-h-0 flex-col gap-2">
      <div className="flex rounded-md border border-neutral-300 self-start dark:border-neutral-700">
        <div className="rounded-l-md overflow-hidden">
          <SubTabButton
            active={subTab === "skills"}
            icon={Sparkles}
            label="Skills"
            onClick={() => setSubTab("skills")}
          />
        </div>
        <div className="rounded-r-md overflow-hidden">
          <SubTabButton
            active={subTab === "scripts"}
            icon={Terminal}
            label="Scripts"
            onClick={() => setSubTab("scripts")}
          />
        </div>
      </div>
      <div className="min-h-0 flex-1">
        {subTab === "skills" ? <SkillWorkspace /> : <ScriptWorkspace />}
      </div>
    </div>
  );
}
