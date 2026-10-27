# Skills and Scripts

The Skills workspace holds two kinds of reusable automation. A **Skill** is a prompt you wrote once and run many times. A **Script** is a Python program you wrote once and run many times.

Both live in the same tab. Open it with `Ctrl/Cmd+5` and switch between the **Skills** and **Scripts** sub-tabs.

| | Skill | Script |
| --- | --- | --- |
| Written in | Markdown with YAML frontmatter | Python with a YAML docstring |
| Does the work by | Sending a prompt to your model | Running a program |
| Runs in | The shell, through the model provider | A separate Python process |
| Needs a provider key | Yes | No |
| Can reach the network | Only through the model provider | Only if you allow it |
| Use it for | Repeated prompts with a few blanks | Computation the model should not do |

## Skills

### What a Skill Looks Like

A skill is a Markdown document. The YAML block at the top is the metadata, and everything below it is the prompt template.

```markdown
---
name: Meeting Summary
description: Turn raw meeting notes into decisions and action items.
category: productivity
parameters:
  - name: notes
    description: The raw notes from the meeting.
    required: true
  - name: format
    description: How to structure the output.
    default: decisions, then action items
---

Summarize the following meeting notes.

Notes:
{{notes}}

Format the output as {{format}}.
```

| Field | Required | What it does |
| --- | --- | --- |
| `name` | Yes | The identifier the skill is listed and saved under. |
| `description` | No | One line describing what the skill does. |
| `category` | No | A grouping label. Defaults to `general`. |
| `parameters` | No | The list of inputs the skill takes. |
| `allow_network` | No | Parsed for both tiers, but ignored for skills. |

Each parameter has a `name`, a `description`, a `required` flag, and an optional `default`. The frontmatter has to open on the first non-blank line of the file and close on a line that is exactly three dashes.

### How the Template Is Filled In

A `{{name}}` placeholder in the body is replaced with the value of the parameter of that name. If the form field is empty:

1. The parameter's default is used.
2. If the parameter is required and has no default, the run stops with an error naming the parameter.
3. If the parameter is optional and has no default, the placeholder becomes an empty string.

A placeholder that does not name a declared parameter is left exactly as it is. That makes `{{date}}` safe to write when you mean it literally.

### Running a Skill

1. Select the skill in the list on the left.
2. Fill in the parameter form. Required parameters are marked.
3. Click **Run Skill**.

The output appears below, rendered as Markdown, with the token counts for the call. Two actions sit next to the result:

* **Clear Output:** Only the model output. Your parameter values stay filled in.
* **Reset Inputs:** The parameter values. The output stays.

Parameter values, the network toggle, and the last result are kept for the session, so switching tabs or opening another item and coming back does not lose what you typed.

### Saving the Output as a Note

Click **Save as Note**. A title field and a project picker appear. The note is created in the Knowledge Base with the author set to `ai`. The confirmation names the destination, so you can see where the note went. The button stays disabled for that exact output, which means a double-click cannot create two identical notes.

### Renaming a Skill

Edit the `name` in the frontmatter and save. The row is renamed in place. If the new name already belongs to another skill, the save is refused and nothing is written.

## Scripts

### What a Script Looks Like

A script is a Python file. The metadata lives in the module docstring, at the very top of the file, and uses the same YAML as a skill plus one extra field.

```python
"""
name: Word Count
description: Count the words in a block of text.
category: text
parameters:
  - name: text
    description: The text to measure.
    required: true
  - name: top
    description: How many frequent words to list.
    default: "10"
allow_network: false
"""

import collections
import json
import os

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
words = params["text"].split()
print(f"{len(words)} words")
for word, count in collections.Counter(words).most_common(int(params["top"])):
    print(f"{word}: {count}")
```

The parameters arrive as a JSON file. The path is in the `ENDLESSLI_INPUT` environment variable.

### How a Script Runs

The script runs in its own process, not inside the app:

| Property | Value |
| --- | --- |
| Interpreter | `python -I -X utf8 -B` |
| Working directory | A new temporary directory, private to the run |
| Timeout | 1 to 600 seconds, 60 by default |
| Output | Capped at 1 MB per stream, stdout and stderr separately |
| Environment | Only the parameters path, the network flag, and the encoding |
| Exit | The script's own exit code, reported in the result |

The run panel shows stdout, stderr, the exit code, the duration, and whether the script timed out.

### The Network Toggle

Scripts can be given network access, and the toggle is **off by default**. A script that declares `allow_network: true` starts with the toggle already on.

**What the Toggle Actually Does:** When it is off, the runner replaces `socket.socket`, `socket.create_connection`, and `socket.getaddrinfo` with versions that refuse to work. A script that tries to open a connection gets a permission error rather than a connection.

**What it Does Not Do:** It is not an operating-system sandbox. It is enforced inside the interpreter, so a script that restores the original functions, reaches for native APIs, or starts another program can still reach the network. And a script runs with your own user permissions, so it can read anything you can read.

The first time you turn the toggle on, the app shows a confirmation that says so plainly. After that it does not interrupt you again. Turn it on for code you wrote or fully trust, and never put a credential in a script.

### Scripts Need a Python Interpreter

A script runs on a Python interpreter, and a packaged build does not ship one. The frozen sidecar in a packaged build is a program, not an interpreter, so scripts cannot run inside it.

The Scripts tab shows a warning when no interpreter is found. In a development build the sidecar environment is used, so scripts work as soon as you have created it. In a packaged build, install Python and make sure it is on `PATH`.

### Editing and Saving a Script

The editor is a Python editor with syntax highlighting. The same validation rules apply as for a skill. Saving with a changed `name` renames the row, exactly as it does for skills.

## The Built-In Items

Six skills and six scripts are seeded the first time the app starts:

| Item | Type | What it does |
| --- | --- | --- |
| Summarize | Skill | Condenses a block of text into a chosen number of bullet points. |
| Explain Simply | Skill | Explains a topic for a chosen audience. |
| Action Items | Skill | Extracts tasks and deadlines into a Markdown checklist. |
| Editor | Skill | Rewrites text based on specific constraints (shorter, active voice, etc). |
| Data to Table | Skill | Extracts entities from text into a clean Markdown table. |
| Steelman This | Skill | Rewrites a weak argument into its strongest, most charitable form. |
| Keyword Extractor | Script | Counts words, estimates reading time, and extracts top keywords. |
| CSV to Markdown | Script | Converts raw CSV text into a clean Markdown table. |
| Basic CSV Cleaner | Script | Removes duplicate rows and rows with empty cells. |
| YouTube Transcript | Script | Fetches the plain text transcript from a YouTube URL. |
| Deduplicator | Script | Removes duplicate lines from a list and optionally sorts them. |
| Randomizer | Script | Takes a list of items and randomly selects one. |

Most scripts are offline. The only exception is the YouTube Transcript script, which declares `allow_network: true`. Seeding only happens when the table is empty. If you delete a built-in, it stays deleted.

## Extending Versus Editing

A skill or script is a document in the local database. Editing one changes it for this installation only. A skill is a Markdown file and a script is a Python file, so either one can be copied out and given to someone else. You can share it in the Discussions tab on GitHub, or keep it in your own personal library.
