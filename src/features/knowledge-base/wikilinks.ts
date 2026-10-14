/** `[[wikilink]]` parsing shared by the editor preview and the links panel. */

const WIKILINK_PATTERN = /\[\[([^\][\n]+)\]\]/g;

/** Returns every distinct `[[title]]` target in reading order. */
export function parseWikilinks(content: string): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const match of content.matchAll(WIKILINK_PATTERN)) {
    const title = match[1].trim();
    if (title && !seen.has(title)) {
      seen.add(title);
      out.push(title);
    }
  }
  return out;
}

/**
 * Rewrites `[[title]]` as Markdown links with a `wikilink:` scheme so the
 * preview renderer can intercept them. Titles are URI-encoded into the href.
 */
export function renderWikilinksAsLinks(content: string): string {
  return content.replace(WIKILINK_PATTERN, (_full, title: string) => {
    const trimmed = title.trim();
    return `[${trimmed}](#wikilink:${encodeURIComponent(trimmed)})`;
  });
}

/** Extracts the title from an intercepted `#wikilink:` href, if present. */
export function wikilinkTitleFromHref(href: string | undefined): string | null {
  if (!href || !href.startsWith("#wikilink:")) return null;
  try {
    return decodeURIComponent(href.slice("#wikilink:".length));
  } catch {
    return null;
  }
}
