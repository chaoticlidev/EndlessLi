import type { MouseEvent } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

/**
 * Schemes the opener capability is scoped to. Keep this in sync with
 * `src-tauri/capabilities/default.json` (`opener:allow-default-urls`).
 */
const ALLOWED_PROTOCOLS = new Set(["http:", "https:", "mailto:", "tel:"]);

/** Returns true only for absolute URLs the system opener is allowed to handle. */
export function isExternalUrl(value: string | null | undefined): boolean {
  if (!value) return false;
  try {
    return ALLOWED_PROTOCOLS.has(new URL(value).protocol);
  } catch {
    return false;
  }
}

/**
 * Opens an allowed URL in the system browser/mail client. Invalid or
 * disallowed URLs are ignored so a crafted note or source cannot hand the
 * opener a `file:`/`javascript:` target.
 */
export async function openExternal(value: string | null | undefined): Promise<void> {
  const url = value?.trim();
  if (!url || !isExternalUrl(url)) return;
  try {
    await openUrl(url);
  } catch (err) {
    console.error("failed to open external URL", err);
  }
}

/**
 * Props for an anchor that opens externally. Non-external targets keep their
 * normal href behavior (relative links, in-app wikilinks).
 */
export function externalLinkProps(value: string | null | undefined): {
  href: string | undefined;
  rel?: string;
  onClick?: (event: MouseEvent<HTMLAnchorElement>) => void;
} {
  if (!isExternalUrl(value)) {
    return { href: value ?? undefined };
  }
  const url = value as string;
  return {
    href: url,
    rel: "noreferrer",
    onClick: (event) => {
      event.preventDefault();
      void openExternal(url);
    },
  };
}
