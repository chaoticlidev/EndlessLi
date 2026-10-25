//! Built-in skills and scripts shipped with the app (task 10.7).
//!
//! The documents are the source of truth (they are what the editor round-trips),
//! so seeding parses them through the very same validators a user's edit hits.
//! Seeding is idempotent and only fills an empty table: once a user has skills
//! or scripts, their set is theirs to keep.

use el_core::{AppResult, NewScript, NewSkill};

use crate::script::{parse_script, to_new_script};
use crate::skill::{parse_skill, to_new_skill};

/// Three example skills covering summarization, explanation, and structuring.
pub const SKILL_DOCUMENTS: &[&str] = &[
    r#"---
name: Summarize
description: Condense a passage into its key points.
category: writing
parameters:
  - name: text
    description: The passage to summarize.
    required: true
  - name: length
    description: Target length, e.g. "3 bullet points" or "one paragraph".
    default: 3 bullet points
---
Summarize the following passage into {{length}}. Preserve names, numbers, and
conclusions, and do not add any information that is not present in the source.

Passage:
{{text}}
"#,
    r#"---
name: Explain Simply
description: Explain a topic in plain language for a general audience.
category: learning
parameters:
  - name: topic
    description: The concept or term to explain.
    required: true
  - name: audience
    description: Who the explanation is for.
    default: a curious adult with no background in the field
---
Explain {{topic}} to {{audience}}. Use one concrete analogy, keep it under 200
words, and finish with a single sentence on why it matters.
"#,
    r#"---
name: Meeting Notes
description: Turn a rough transcript into clean, structured meeting notes.
category: productivity
parameters:
  - name: notes
    description: The raw notes or transcript.
    required: true
---
Turn the following raw notes into clean meeting notes with three sections:
Summary, Decisions, and Action Items. List each action item with its owner when
one is named in the source.

Raw notes:
{{notes}}
"#,
];

/// Two example scripts, both offline: one text analysis, one data summary.
pub const SCRIPT_DOCUMENTS: &[&str] = &[
    r#""""
name: Text Stats
description: Report word, line, and character counts plus the most frequent words.
category: text
parameters:
  - name: text
    description: The text to analyze.
    required: true
  - name: top
    description: How many frequent words to show.
    default: "10"
allow_network: false
"""

import json
import os
import re
from collections import Counter

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
text = params.get("text", "")
top = int(params.get("top") or 10)

words = re.findall(r"[A-Za-z0-9']+", text.lower())
counts = Counter(words)

print("| Metric | Value |")
print("| --- | --- |")
print(f"| Characters | {len(text)} |")
print(f"| Words | {len(words)} |")
print(f"lines: {text.count(chr(10)) + 1}")
print(f"unique words: {len(counts)}")
print()
print(f"top {min(top, len(counts))} words:")
for word, count in counts.most_common(top):
    print(f"  {word}: {count}")
"#,
    r#""""
name: CSV Summary
description: Summarize CSV text with row, column, and per-column statistics.
category: data
parameters:
  - name: csv
    description: The CSV text, including a header row.
    required: true
allow_network: false
"""

import csv
import io
import json
import os

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
rows = list(csv.reader(io.StringIO(params.get("csv", ""))))
if not rows:
    print("no data")
    raise SystemExit(1)

header, *body = rows
print(f"columns: {len(header)}")
print(f"rows: {len(body)}")
for index, column in enumerate(header):
    values = [row[index] for row in body if index < len(row)]
    numbers = []
    for value in values:
        try:
            numbers.append(float(value))
        except ValueError:
            pass
    if numbers and len(numbers) == len(values):
        mean = sum(numbers) / len(numbers)
        print(f"{column}: numeric min={min(numbers)} max={max(numbers)} mean={mean:.2f}")
    else:
        print(f"{column}: {len(set(values))} unique value(s)")
"#,
];

/// Parses and converts every built-in skill into a repository input.
pub fn builtin_skills() -> AppResult<Vec<NewSkill>> {
    SKILL_DOCUMENTS
        .iter()
        .map(|document| {
            let doc = parse_skill(document)?;
            to_new_skill(document, &doc)
        })
        .collect()
}

/// Parses and converts every built-in script into a repository input.
pub fn builtin_scripts() -> AppResult<Vec<NewScript>> {
    SCRIPT_DOCUMENTS
        .iter()
        .map(|document| {
            let doc = parse_script(document)?;
            to_new_script(document, &doc)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_parse_and_are_valid() {
        let skills = builtin_skills().expect("skills parse");
        assert_eq!(skills.len(), 3);
        let scripts = builtin_scripts().expect("scripts parse");
        assert_eq!(scripts.len(), 2);
    }

    #[test]
    fn builtins_have_distinct_names() {
        let skills = builtin_skills().expect("skills parse");
        let unique: std::collections::HashSet<_> = skills.iter().map(|s| &s.name).collect();
        assert_eq!(unique.len(), skills.len());
        let scripts = builtin_scripts().expect("scripts parse");
        let unique: std::collections::HashSet<_> = scripts.iter().map(|s| &s.name).collect();
        assert_eq!(unique.len(), scripts.len());
    }

    #[test]
    fn builtin_scripts_are_offline_by_default() {
        let scripts = builtin_scripts().expect("scripts parse");
        assert!(scripts.iter().all(|script| !script.allow_network));
    }
}
