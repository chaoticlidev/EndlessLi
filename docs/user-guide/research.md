# Research

The Research workspace answers a question with sources. It runs a web search, reads the pages it finds, asks your model to synthesize an answer from them, and keeps the URLs so the answer can be traced back to what it came from.

Two modes are available: Light Search is one pass. Deep Dive plans first, then runs several passes and merges the results.

## Light Search

Light Search is the right default. It is one search, a handful of page fetches, and one model call, so it finishes in seconds and costs a fraction of a Deep Dive.

1. Open Research with `Ctrl/Cmd+2`.
2. Type the question in the composer at the bottom of the window.
3. Leave the mode on **Light Search**.
4. Click **Send**.

The run shows its steps as they happen:

* **Search:** The query goes to the configured search provider and the top hits come back.
* **Clip Top Pages:** The leading results are fetched and reduced to readable text.
* **Synthesize Answer:** The model writes an answer using only the clipped text, with numbered citations.

The answer appears as a result card. Under it you find one card per source, with the page title, the host, and a short excerpt.

Running the same query again is fast. Search results are cached for 24 hours and clipped pages for 7 days, so a repeat run usually skips straight to the synthesis.

## Deep Dive

Deep Dive is for a question with several parts, where one pass is likely to miss something.

1. Type the question in the composer.
2. Choose **Deep Dive**.
3. Click **Send**.

The run works in three stages:

1. **Plan:** The model turns your question into at most five sub-questions.
2. **Child Passes:** Each sub-question runs a full light pass. Two run at a time, so a five-part question takes roughly three rounds.
3. **Merge:** The child answers are merged into one answer with the citation numbers rewritten onto a single list of sources, and a **Cross-source Insights** section at the end.

A Deep Dive costs several times more tokens than a Light Search. The token counters under the session show the running total.

Two things to know about the planner:

* If the planner returns nothing usable, the run degrades to a single light pass, and the plan step is marked as completed.
* If one child pass fails, the others still finish. The merged answer covers what succeeded.

## Stopping a Run

Click **Stop** while a run is in progress. Stop lands during a long request, not only between steps. The status becomes `cancelled`. The steps that finished stay visible, and whatever results were produced are kept. A cancelled session is still in the history, and you can reopen it.

## Reading the Results

The **summary** result is the model's answer. Read it as a report rather than as settled fact, because a model that reads a page can still misread it.

* **Citations are validated.** If the model cites a source number that does not exist, the marker is removed. If no valid citation survives at all, the summary ends with a note saying the claims are unverified.
* **The sources are the evidence.** Each source card links to the page that was actually read.
* **Clipping can fail.** A page that needs a login, or that serves almost no text, is dropped from the source list. The run continues with what it could read.

## Saving a Session as a Note

Nothing from a research run reaches your library until you save it.

1. Tick the result cards you want.
2. Click **Save**. The save dialog opens.
3. Give the note a title.
4. Pick the destination (Inbox is the default).
5. Click **Save Note**.

The note contains the selected summaries inline, a **Sources** section listing every source with its URL, and a provenance line at the end that records the research session it came from.

## Adding Sources to the Library

The research results can feed the Library directly.

* **Add All to Library** copies every source of the run into the Library.
* The per-source button on a result card copies that one source.

The copy uses the text that was already clipped for the research run, so it usually works even if the page is not reachable at that moment. Items are deduplicated by URL, so adding the same source twice does not create a duplicate.

## History

The History panel on the left lists past sessions, grouped by day. Selecting one reopens it with its steps and its results intact.

Each session can be deleted from the panel. Deleting a session removes the run and its results, but the notes that were saved from it stay in the Knowledge Base. A session that was interrupted shows as `failed` when you open it. That is the app cleaning up a run that has no process behind it, not a new failure.

## Session Memory

A research run can carry a short piece of context of its own. It is entered as part of the run and applies to that session only. Global and project memories are applied to every run, and they are managed in Settings. See [Memory](./memory.md).

## Managing Research Cost

Three settings in the Settings modal control cost:

* **Daily Token Limit:** Stops new model calls once the rolling 24-hour total reaches the cap.
* **Monthly Token Limit:** The same over a rolling 30-day window.
* **Cache Size:** How much search and clipping results are kept.

At 80 percent of a limit you get a warning. At the limit, new calls are refused before they are sent, and the message tells you how much of the budget is used.
