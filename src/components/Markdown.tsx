import type { ComponentProps } from "react";
import ReactMarkdown from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import remarkGfm from "remark-gfm";
import { externalLinkProps } from "../lib/openExternal";

type MarkdownComponents = ComponentProps<typeof ReactMarkdown>["components"];

/**
 * Shared sanitized Markdown renderer. Anchors default to the scoped system
 * opener; callers can override any component (the note editor replaces `a`
 * to resolve `[[wikilinks]]`).
 */
export function Markdown({
  content,
  components,
}: {
  content: string;
  components?: MarkdownComponents;
}) {
  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      rehypePlugins={[rehypeSanitize]}
      components={{
        a: ({ href, children }) => <a {...externalLinkProps(href)}>{children}</a>,
        ...components,
      }}
    >
      {content}
    </ReactMarkdown>
  );
}
