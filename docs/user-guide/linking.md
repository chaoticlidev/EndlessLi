# Linking Notes

Notes in EndlessLi can be linked to each other. A link is bidirectional and
stored once, so it shows up on both notes and cannot drift into two separate
records.

There are two ways to create one. You can link notes by hand, or you can let
the app propose links based on what the notes say, and approve the ones that
make sense.

## Auto-Linking

Auto-linking runs on your machine. After you save a note, the app embeds its
text locally, compares it against the other notes you have already embedded,
and proposes the closest matches. Nothing is linked until you approve it.

### Turning It On

Auto-linking needs the optional embedding backend. The base install does not
include it, because it is a large download that only some people need.

Install the extra into the sidecar environment:

On Windows:

```powershell
sidecar\.venv\Scripts\python.exe -m pip install -r sidecar\requirements-embeddings.txt
```

On macOS or Linux:

```bash
sidecar/.venv/bin/python -m pip install -r sidecar/requirements-embeddings.txt
```

Restart the app. Open Settings with Ctrl/Cmd+5 and look at the Embedding
section.

You should see:

| Field | What it Means |
| --- | --- |
| Model | The embedding model in use |
| Dimensions | The size of the vector, which has to match the model |
| Coverage | How many of your notes have a vector, as a bar and a count |
| Auto-link threshold | The minimum similarity for a suggestion |

The model weights download the first time they are needed and are then cached
in the app data directory under `models/`. The first embedding is therefore
slower than the ones after it.

If the Embedding section says the backend is not installed, the extra was
not installed into the environment the app is actually using. See
[Troubleshooting](./troubleshooting.md).

### How a Suggestion Appears

1. Save a note. The editor saves on its own a moment after you stop typing.
2. The app waits about a second and a half, then embeds the note.
3. The links panel at the bottom of the editor shows a **Suggested links**
   section with the closest matches.
4. Each suggestion shows the note title and a percentage, which is the
   similarity between the two notes.

Suggestions only refresh for the note you just saved. If you later write
something that makes an older note more similar, that older note does not
push a refreshed suggestion list until it is embedded again or you reopen it.

### Approving a Suggestion

Click the check mark next to the suggestion. The note moves into the
**Linked notes** section above, and the link is permanent and bidirectional.
The link appears on both notes from that moment, and it survives a restart.

### Rejecting a Suggestion

Click the cross next to the suggestion. It disappears and is not offered
again for that pair, not now and not after a restart.

Rejection is permanent on purpose. A suggestion you rejected once is a
decision you already made, and the app does not ask again.

### What Is Never Suggested

Four exclusions, all enforced in the query rather than in the interface:

| Excluded | Why |
| --- | --- |
| The note itself | A note is trivially identical to itself. |
| Notes already linked | The link exists. |
| Pairs you rejected | You already said no. |
| Notes embedded with a different model | Vectors from different models are not comparable. |

At most five candidates are returned for one note.

## Adjusting the Threshold

The threshold is the minimum cosine similarity a candidate has to reach
before it is offered. The default is 0.75.

It lives in Settings, under Embedding, in the field labelled **Auto-link
threshold (0-1)**. The value is clamped to the range 0 through 1. Values
outside the range, and values that are not a number, fall back to the
default.

| Threshold | What Happens |
| --- | --- |
| 0.9 and above | Only near-duplicates are suggested. Expect nothing most of the time. |
| 0.75 | The default. Strict for the small models, and it produces few suggestions on a mixed corpus. |
| 0.5 to 0.7 | The useful range for a small embedding model. Related notes that share a topic start to appear. |
| 0.3 and below | Everything remotely on the same subject appears. Expect noise. |

The recommendation is to start at 0.5 if suggestions feel sparse, and only
raise it if unrelated notes keep appearing. The default of 0.75 is high
because a note about a grocery budget should not be linked to a note about
sourdough just because both are long.

Changing the threshold takes effect on the next suggestion. Nothing is
re-embedded and no note is touched.

## Changing the Embedding Model

The model dropdown in the Embedding section lists the available models:

| Model | Dimensions |
| --- | --- |
| MiniLM L6 v2 | 384 |
| BGE small en v1.5 | 384 |
| BGE base en v1.5 | 768 |
| Arctic embed s | 384 |
| Arctic embed m | 768 |

A custom model name can be typed in as well.

Switching models re-embeds everything. The vector table is dropped and
recreated because vectors from different models are not comparable, and every
note is queued for embedding again. On a large library that takes a while.
Watch the coverage bar: it falls to zero and climbs back as the background
worker catches up.

## Coverage and Re-Embedding

The coverage bar shows how many notes have a vector for the current model.
Notes are embedded only when their text has changed, so coverage stays at 100
percent unless something interrupts the work.

**Re-embed missing notes** queues every note that does not have a current
vector. Use it after installing the embedding backend on an existing
library, or after a run that was interrupted.

Encrypted projects are excluded. Notes in an encrypted project are never
embedded, because the vector table holds plaintext and embedding them would
leak the content the encryption is there to protect. This also means
encrypted notes never appear as suggestions.

## Linking Notes by Hand

The links panel has a search field for adding a link directly. Type a title,
pick the note from the results, and the link is created with the origin
marked as manual. A manual link is never affected by the threshold.

Two notes are also linked automatically when you type a wikilink in the note
body, for example `[[Sourdough starter]]`. The link renders in the preview.
The title is resolved by an exact match first and a prefix match second, and
the resolved note is added to the links list.

Wikilink resolution by title does not match encrypted notes, because their
titles are stored as ciphertext. The wikilink still renders; it just does not
point anywhere.

## Removing a Link

Click the remove button next to a link in the links panel. The link is
removed from both notes. Nothing else happens, and the pair is not excluded
from future suggestions, so it may be proposed again.

## Related

- [Getting Started](./getting-started.md) covers installing the embedding
  backend.
- [Troubleshooting](./troubleshooting.md) covers an embedding backend that
  is missing or a coverage bar that does not move.
