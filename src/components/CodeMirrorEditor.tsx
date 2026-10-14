import { useEffect, useRef } from "react";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { defaultKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";

interface CodeMirrorEditorProps {
  initialDoc: string;
  onChange: (doc: string) => void;
  autofocus?: boolean;
}

/**
 * Minimal CodeMirror 6 Markdown editor. The parent owns the document: it
 * passes the opening text once and receives edits through `onChange`.
 * External resets (switching notes) are handled by remounting via `key`.
 * The editor is created exactly once per mount; later `initialDoc` values
 * are intentionally ignored so typing never rebuilds the view.
 */
export function CodeMirrorEditor({
  initialDoc,
  onChange,
  autofocus,
}: CodeMirrorEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const onChangeRef = useRef(onChange);
  const mountedRef = useRef(false);

  useEffect(() => {
    onChangeRef.current = onChange;
  });

  useEffect(() => {
    if (mountedRef.current) return undefined;
    mountedRef.current = true;
    const host = hostRef.current;
    if (!host) return undefined;

    const state = EditorState.create({
      doc: initialDoc,
      extensions: [
        keymap.of(defaultKeymap),
        markdown(),
        EditorView.lineWrapping,
        EditorView.updateListener.of((update) => {
          if (update.docChanged) {
            onChangeRef.current(update.state.doc.toString());
          }
        }),
      ],
    });
    const view = new EditorView({ state, parent: host });
    if (autofocus) view.focus();

    return () => {
      mountedRef.current = false;
      view.destroy();
    };
  }, [autofocus, initialDoc]);

  return <div ref={hostRef} className="cm-host min-h-64 text-sm" />;
}
