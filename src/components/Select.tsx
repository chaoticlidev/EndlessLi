import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown } from "lucide-react";

export interface SelectOption {
  value: string;
  label: string;
}

interface SelectProps {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  ariaLabel?: string;
  disabled?: boolean;
  size?: "sm" | "md";
  pill?: boolean;
  /** Applied to the positioning wrapper (e.g. width). */
  className?: string;
  /** Extra classes on the popup. */
  menuClassName?: string;
  placeholder?: string;
}

/**
 * A small custom dropdown, used instead of `<select>` so options are readable
 * in dark mode and the menu matches the app's surface styling.
 *
 * The menu is portalled to `document.body` and positioned with fixed
 * coordinates so it is never clipped by a scrolling drawer or settings pane.
 * It flips above the trigger when there is not enough room below.
 *
 * Keyboard: the trigger is a normal button; Tab/Enter work, Escape closes, and
 * clicking anywhere outside closes the menu.
 */
export function Select({
  value,
  onChange,
  options,
  ariaLabel,
  disabled,
  size = "md",
  pill = false,
  className,
  menuClassName,
  placeholder = "Select…",
}: SelectProps) {
  const [open, setOpen] = useState(false);
  const [rect, setRect] = useState<DOMRect | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    function onPointerDown(event: PointerEvent) {
      const target = event.target as Node;
      if (
        !rootRef.current?.contains(target) &&
        !menuRef.current?.contains(target)
      ) {
        setOpen(false);
      }
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    function close() {
      setOpen(false);
    }
    function onScroll(event: Event) {
      // Scrolling inside the menu itself must not dismiss it.
      if (
        event.target instanceof Node &&
        menuRef.current?.contains(event.target)
      ) {
        return;
      }
      setOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", close);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", close);
    };
  }, [open]);

  const selected = options.find((option) => option.value === value);
  const shape = pill ? "rounded-full" : "rounded-md";
  const sizing =
    size === "sm" ? "px-2.5 py-1 text-xs" : "px-2 py-1.5 text-sm";

  const menu = (() => {
    if (!open || !rect) return null;
    const below = window.innerHeight - rect.bottom;
    const above = rect.top;
    const openUp = below < 180 && above > below;
    const maxHeight = Math.min(256, Math.max(120, (openUp ? above : below) - 12));
    return createPortal(
      <div
        ref={menuRef}
        role="listbox"
        aria-label={ariaLabel}
        style={{
          position: "fixed",
          left: rect.left,
          minWidth: rect.width,
          maxWidth: Math.min(320, window.innerWidth - 16),
          maxHeight,
          ...(openUp
            ? { bottom: window.innerHeight - rect.top + 4 }
            : { top: rect.bottom + 4 }),
        }}
        className={`z-50 overflow-y-auto rounded-md border border-neutral-300 bg-white p-1 shadow-lg dark:border-neutral-600 dark:bg-neutral-900 ${
          menuClassName ?? ""
        }`}
      >
        {options.length === 0 && (
          <p className="px-2 py-1 text-xs text-neutral-500">No options</p>
        )}
        {options.map((option) => {
          const active = option.value === value;
          return (
            <button
              key={option.value}
              type="button"
              role="option"
              aria-selected={active}
              onClick={() => {
                onChange(option.value);
                setOpen(false);
              }}
              className={`flex w-full items-center justify-between gap-2 rounded px-2 py-1 text-left text-sm ${
                active
                  ? "bg-neutral-100 dark:bg-neutral-800"
                  : "hover:bg-neutral-100 dark:hover:bg-neutral-800"
              }`}
            >
              <span className="truncate">{option.label}</span>
              {active && <Check size={13} className="shrink-0" />}
            </button>
          );
        })}
      </div>,
      document.body,
    );
  })();

  return (
    <div ref={rootRef} className={`relative ${className ?? ""}`}>
      <button
        type="button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label={ariaLabel}
        disabled={disabled}
        onClick={() => {
          if (open) {
            setOpen(false);
            return;
          }
          setRect(rootRef.current?.getBoundingClientRect() ?? null);
          setOpen(true);
        }}
        className={`flex w-full items-center justify-between gap-2 ${shape} border border-neutral-300 bg-white ${sizing} text-left text-neutral-900 outline-none disabled:opacity-50 dark:border-neutral-600 dark:bg-neutral-900 dark:text-neutral-100`}
      >
        <span className="truncate">{selected?.label ?? placeholder}</span>
        <ChevronDown
          size={14}
          className={`shrink-0 text-neutral-400 transition-transform ${
            open ? "rotate-180" : ""
          }`}
        />
      </button>
      {menu}
    </div>
  );
}
