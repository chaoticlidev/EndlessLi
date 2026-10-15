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
 * Switching notes remounts via `key` from the parent.
 *
 * The editor is created exactly once per mount. The effect depends only on
 * stable values on purpose: `initialDoc` changes on every keystroke (it is
 * the live parent state), and re-running creation there would destroy and
 * recreate the view — and with it the user's focus — on each keystroke.
 */
export function CodeMirrorEditor({
  initialDoc,
  onChange,
  autofocus,
}: CodeMirrorEditorProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const initialRef = useRef(initialDoc);
  const onChangeRef = useRef(onChange);

  useEffect(() => {
    onChangeRef.current = onChange;
  });

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return undefined;

    const state = EditorState.create({
      doc: initialRef.current,
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

    return () => view.destroy();
    // Mount-once by design; see the doc comment above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return <div ref={hostRef} className="cm-host min-h-64 text-sm" />;
}
