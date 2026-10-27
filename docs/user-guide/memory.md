# Memory

Memory is what the assistant is told before it does something. It is a list
of statements you wrote down, and it is yours to read, edit, and remove. The
app never adds a memory on its own, and a statement you write can be turned
off in one click.

Memories are inserted into the prompt of every model call: research runs,
skill runs, and the compression pipeline.

## The Two Scopes

| Scope | Applies To | Set it Up Inside |
| --- | --- | --- |
| Global | Every model call, in every project | Settings, or `/remember` in the command palette |
| Project | Model calls made for one project | Settings, or `/project remember` in the command palette |

Project memories are appended after global ones, so a project statement is
the one the model reads last when the two overlap. If both say the same
thing differently, the project one wins.

A research run can also carry a short piece of context for that run alone.
That session context is applied last of all, so it is the strongest signal
for a single question.

## The Command Palette

Press Ctrl/Cmd+K to open the palette. Typing a slash command turns it into a
memory action, and the action appears in the list below the input. Select it
with the arrow keys and press Enter.

| Command | What it Does |
| --- | --- |
| `/remember <text>` | Creates a global memory from the text after the command. |
| `/project remember <text>` | Creates a project memory. The palette asks which project. |
| `/forget <id>` | Hides the memory with that id. It can be restored later. |
| `/showmem` | Opens a modal listing the global memories. |

The palette only offers an action when the rest of the command is present.
`/remember` on its own offers nothing, because there is nothing to remember.

You need the memory id to forget something from the palette. Turn on **Show
memory IDs** in the Settings panel to see them, or forget from Settings
instead, which does not need an id.

The `/project remember` command asks for a project id rather than a project
name. The Settings panel is the easier route when you are not sure of the
id.

## The Settings Panel

Open Settings with Ctrl/Cmd+5 and scroll to the Memories section.

### Turning Memory Off

The **Memory enabled** checkbox is a global kill switch. With it off, no
memory is inserted into any prompt, and the existing memories stay in place
untouched. Turn it back on and they apply again. This is the fastest way to
check whether a memory is causing a bad answer.

### Showing Memory IDs

The **Show memory IDs** checkbox adds the identifier next to each memory.
You need it for the palette commands, and it is the only way to tell two
memories with the same text apart.

### Creating a Memory

1. Choose the scope in the first dropdown: **Global** or **Project**.
2. If you chose Project, pick the project in the dropdown that appears.
3. Type the statement in the text field.
4. Press **Save memory**.

Write memories as statements, not as questions. "The user prefers metric
units" is useful to the model. "What units does the user prefer?" is a
question the model will answer rather than obey.

### Moving a Memory Between Scopes

Each memory has a scope dropdown. Pick **Global** to make it apply
everywhere, or pick a project to scope it to that project. The change takes
effect immediately, with no need to delete and retype the memory.

### Forgetting and Deleting

Two different actions, and the difference matters.

| Action | What happens | Can it be undone |
| --- | --- | --- |
| **Forget** | The memory is hidden and stops being applied. The record is kept. | Yes. It appears under **Forgotten memories** with a **Restore** button. |
| **Delete** | The memory is removed permanently. | No. |

Both ask for confirmation before they act.

The **Forgotten memories** list at the bottom of the section shows everything
that was forgotten but not deleted, and each entry has a **Restore** button.
This is the same list `/forget` writes to.

### Expiry

A memory record can carry an expiry date, after which it stops being applied
and gets swept out of the active list. Memories created from the Settings
panel or the command palette do not have one, so they apply until you forget
or delete them.

## What a Memory Does to a Prompt

The prompt the model receives carries the memories under labelled sections,
in this order:

```text
[Global memory]
- The user writes in metric units.
- The user's notes are written in British English.

[Project memory]
- The audience for this project is a non-specialist.

[Session context]
- Focus on the cost comparison for the European market.
```

The labels are part of the prompt, so the model can tell a standing
instruction from the context of one question.

## Suggested Memories

The model can propose a memory rather than write one. Suggestions wait in a
separate queue and never apply until you accept one.

The suggestion queue is not surfaced in the interface today, so in practice
a memory enters the system only when you write it. Accepting a suggestion
turns it into a normal memory with the source marked as a suggestion, and
rejecting one discards it.

## Things Worth Remembering

A memory that helps is short, specific, and durable.

Good:

- "The user is a researcher in computational biology, not a clinician."
- "Summaries should be in bullet points, no more than five."
- "Never suggest Rust libraries; the user's stack is Python."

Not useful:

- "The user is smart." The model cannot act on this.
- "Use Python." Too broad, and it will fight your other memories.
- Anything that changes next week. Put that in a note instead.

## Privacy

Memories live in the local database, in the same file as your notes. They
are not sent anywhere on their own. They are included in a prompt when a
model call happens, which means their text goes to the provider you
configured, the same way the rest of the prompt does.

A memory is a good place to put a preference you do not want to restate. It
is a bad place to put a secret you would not paste into any chat window,
because it is sent with every model call.

## Related

- [Research](./research.md) uses session context and project memory.
- [Troubleshooting](./troubleshooting.md) covers a memory that behaves
  unexpectedly.
