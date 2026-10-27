//! Built-in skills and scripts shipped with the app (task 10.7).
//!
//! The documents are the source of truth (they are what the editor round-trips),
//! so seeding parses them through the very same validators a user's edit hits.
//! Seeding is idempotent and only fills an empty table: once a user has skills
//! or scripts, their set is theirs to keep.

use el_core::{AppResult, NewScript, NewSkill};

use crate::script::{parse_script, to_new_script};
use crate::skill::{parse_skill, to_new_skill};

// --- SKILLS ---

/// Six native skills, powered by the LLM.
/// These transform text, analyze data, and structure thoughts.
pub const SKILL_DOCUMENTS: &[&str] = &[
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
Explain {{topic}} to {{audience}}. Use one concrete analogy, keep it under 200 words, and finish with a single sentence on why it matters.
"#,
    r#"---
name: Summarize
description: Condense a passage into its key points.
category: text
parameters:
  - name: text
    description: The text to summarize.
    required: true
---
Summarize the following text. Keep the summary concise, focusing only on the most important facts, arguments, and conclusions. Do not add any new information.

Text:
{{text}}
"#,
    r#"---
name: Action Items
description: Extract every actionable task, deadline, or assigned responsibility into a checklist.
category: text
parameters:
  - name: text
    description: The text (e.g., meeting notes, email) to extract tasks from.
    required: true
---
Analyze the text below. Extract every actionable task, deadline, or assigned responsibility. 
Output them ONLY as a Markdown checklist. Use the format: `- [ ] Task (Assigned to: Name, Due: Date)` if the details are present.

Text:
{{text}}
"#,
    r#"---
name: Editor
description: Rewrite text based on specific constraints (e.g., shorter, more direct).
category: text
parameters:
  - name: text
    description: The text to edit.
    required: true
  - name: instructions
    description: How to rewrite the text.
    default: "30% shorter, more direct, and active-voice, but keep all technical jargon intact"
---
Rewrite the following text according to these instructions: {{instructions}}.

Text:
{{text}}
"#,
    r#"---
name: Data to Table
description: Extract key entities from text into a Markdown table.
category: text
parameters:
  - name: text
    description: The text to extract data from.
    required: true
---
Read the following text and extract the key entities and their attributes. 
Output ONLY a Markdown table with the following columns: `Entity | Description | Relevance`. Do not include any introductory or concluding text.

Text:
{{text}}
"#,
    r#"---
name: Steelman This
description: Rewrites a weak argument into its strongest, most charitable form.
category: learning
parameters:
  - name: argument
    description: The text or argument to steelman.
    required: true
---
Read the following argument:
"{{argument}}"

Rewrite it as a "steelman"—the most charitable, logically sound, and well-reasoned version of this argument possible. Remove logical fallacies, strengthen the premises, and make it persuasive to a neutral observer.
"#,
];

// --- SCRIPTS ---

/// Six native scripts, running locally on the user's machine.
/// These are deterministic tasks (math, file parsing, formatting) to save API tokens.
pub const SCRIPT_DOCUMENTS: &[&str] = &[
    r#""""
name: Keyword Extractor
description: Calculates reading time, word count, and extracts top keywords.
category: text
parameters:
  - name: text
    description: The text to analyze.
    required: true
  - name: top_n
    description: Number of keywords to return.
    default: "10"
allow_network: false
"""

import json
import os
import re
from collections import Counter

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
text = params.get("text", "")
top_n = int(params.get("top_n") or 10)

# Built-in basic stopwords to avoid dependencies
STOPWORDS = set(["the", "and", "is", "in", "it", "to", "of", "for", "on", "with", "as", "this", "that", "are", "was", "be", "at", "by", "from", "or", "an", "but", "not", "you", "we", "they", "he", "she", "it's", "i", "a"])

words = re.findall(r"[A-Za-z0-9']+", text.lower())
filtered_words = [w for w in words if w not in STOPWORDS and len(w) > 2]

# Stats
word_count = len(words)
char_count = len(text)
# Standard reading time: 200 words per minute
reading_time = max(1, round(word_count / 200))

print("| Metric | Value |")
print("| --- | --- |")
print(f"| Word count | {word_count} |")
print(f"| Character count | {char_count} |")
print(f"| Estimated reading time | ~{reading_time} min |")
print()
print("Top Keywords:")
counts = Counter(filtered_words)
for word, count in counts.most_common(top_n):
    print(f"- {word} ({count})")
"#,
    r#""""
name: CSV to Markdown
description: Converts a CSV file into a clean Markdown table.
category: data
parameters:
  - name: csv_content
    description: The raw CSV text.
    required: true
allow_network: false
"""

import json
import os
import csv
import io

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
csv_data = params.get("csv_content", "")

reader = csv.reader(io.StringIO(csv_data))
rows = list(reader)
if not rows:
    print("No data found.")
else:
    # Build Markdown header
    headers = rows[0]
    print("| " + " | ".join(headers) + " |")
    print("|" + "|".join(["---"] * len(headers)) + "|")
    
    # Build rows
    for row in rows[1:]:
        # Pad row if it's shorter than headers
        padded = row + [""] * (len(headers) - len(row))
        print("| " + " | ".join(padded[:len(headers)]) + " |")
"#,
    r#""""
name: Basic CSV Cleaner
description: Removes duplicate rows and drops any row containing an empty cell.
category: data
parameters:
  - name: csv_content
    description: The raw CSV text.
    required: true
allow_network: false
"""

import json
import os
import csv
import io

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
csv_data = params.get("csv_content", "")

reader = csv.reader(io.StringIO(csv_data))
rows = list(reader)

if not rows:
    print("No data found.")
    raise SystemExit(1)

header = rows[0]
body = rows[1:]

# Drop duplicates and rows with empty cells
seen = set()
cleaned_body = []

for row in body:
    # Pad row to match header length
    padded_row = row + [""] * (len(header) - len(row))
    
    # Drop if any cell is empty
    if any(cell.strip() == "" for cell in padded_row[:len(header)]):
        continue
        
    row_tuple = tuple(padded_row[:len(header)])
    if row_tuple not in seen:
        seen.add(row_tuple)
        cleaned_body.append(padded_row[:len(header)])

# Output as CSV
out = io.StringIO()
writer = csv.writer(out)
writer.writerow(header)
writer.writerows(cleaned_body)
print(out.getvalue())
"#,
    r#""""
name: YouTube Transcript
description: Extracts the plain text transcript from a YouTube video URL.
category: text
parameters:
  - name: url
    description: The YouTube video URL.
    required: true
allow_network: true
"""

import json
import os
import re

# Note: This script requires youtube-transcript-api
# Ensure it is added to your Python sidecar/venv requirements
try:
    from youtube_transcript_api import YouTubeTranscriptApi
except ImportError:
    print("Error: youtube-transcript-api is not installed.")
    raise SystemExit(1)

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
url = params.get("url", "")

# Extract Video ID from URL
video_id_match = re.search(r"(?:v=|\/)([0-9A-Za-z_-]{11}).*", url)
if not video_id_match:
    print("Error: Could not extract YouTube video ID from URL.")
    raise SystemExit(1)

video_id = video_id_match.group(1)

try:
    # New API: Instantiate the class, then call fetch
    ytt_api = YouTubeTranscriptApi()
    transcript = ytt_api.fetch(video_id)
    
    # The new API returns objects with a .text attribute, not a dict
    full_text = " ".join([snippet.text for snippet in transcript])
    # Clean up newlines
    full_text = full_text.replace("\n", " ")
    print(full_text)
except Exception as e:
    print(f"Error fetching transcript: {e}")
"#,
    r#""""
name: Deduplicator
description: Removes duplicate lines from a list and optionally sorts them.
category: text
parameters:
  - name: text
    description: The list of items (one per line).
    required: true
  - name: sort
    description: Set to "true" to sort alphabetically.
    default: "false"
allow_network: false
"""

import json
import os

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
text = params.get("text", "")
sort_val = params.get("sort", False)
# Handle both string "true" and boolean True
do_sort = str(sort_val).lower() == "true"

lines = text.splitlines()
cleaned_lines = []
seen = set()

for line in lines:
    stripped = line.strip()
    if stripped and stripped not in seen:
        seen.add(stripped)
        cleaned_lines.append(stripped)

if do_sort:
    cleaned_lines.sort()

for line in cleaned_lines:
    print(line)
"#,
    r#""""
name: Randomizer
description: Takes a list of items and randomly selects one.
category: text
parameters:
  - name: text
    description: The list of items (one per line).
    required: true
allow_network: false
"""

import json
import os
import random

params = json.load(open(os.environ["ENDLESSLI_INPUT"], encoding="utf-8"))
text = params.get("text", "")

lines = [line.strip() for line in text.splitlines() if line.strip()]

if not lines:
    print("Error: No items provided.")
else:
    choice = random.choice(lines)
    print(f"**Selected:** {choice}")
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
        assert_eq!(skills.len(), 6);
        let scripts = builtin_scripts().expect("scripts parse");
        assert_eq!(scripts.len(), 6);
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
    fn builtin_scripts_are_mostly_offline() {
        let scripts = builtin_scripts().expect("scripts parse");
        // Only the YouTube Transcript script is allowed to use the network.
        let online: Vec<&str> = scripts
            .iter()
            .filter(|script| script.allow_network)
            .map(|script| script.name.as_str())
            .collect();
        assert_eq!(
            online,
            vec!["YouTube Transcript"],
            "Unexpected online scripts: {:?}",
            online
        );
    }
}
