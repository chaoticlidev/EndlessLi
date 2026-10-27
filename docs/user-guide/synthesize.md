# Synthesize

The Synthesize workspace is built for a specific problem: you have a pile of documents (PDFs, articles, notes) and you need to extract the core, non-overlapping themes from them. It uses **Semantic Compression** to find the signal in the noise.

Open it with `Ctrl/Cmd+4`.

## How It Works

Traditional search finds exact word matches. Synthesize uses **K-Means Clustering** to find meaning.

1. **Chunking:** The app breaks your documents into smaller chunks of text.
2. **Embedding:** It turns each chunk into a vector (a list of numbers representing meaning). Chunks about the same topic end up close together in this mathematical space.
3. **Clustering (K-Means):** It groups these chunks into clusters of similar meaning.
4. **Generation:** It picks the most representative chunk from each cluster and asks the AI to generate a specific output (like a report or a flashcard) based on that core theme.

Because the AI is only looking at the *representative* chunks, it doesn't waste tokens repeating the same information. It generates K distinct outputs that cover the breadth of your source material.

## Starting a Synthesis

1. Click **New Synthesis** in the center of the workspace (or in the History panel on the left).
2. **Add Documents:** You can drag and drop files, click **Import Files**, or select existing notes from your Knowledge Base or Library.
    * Supported file types: PDF, Markdown, Text, CSV, JSON.
    * *Note: Importing a file here adds a managed copy to your Library automatically so you can reference it later.*
3. **Choose Output Type:** Select what you want to generate.
    * **Report:** A comprehensive written summary.
    * **Slides:** An outline structured for presentation (currently outputs Markdown).
    * **Flashcards:** A Q&A format.
    * **Quiz:** A multi-choice test with an answer key.
    * **Audio:** An MP3 file generated via text-to-speech. *(Requires a TTS engine configured in Settings)*.
    * **Dataset:** Extracts structured rows from a CSV.
4. **Choose "How Many?" (K):** Select a number from 1 to 20. This is the number of outputs you want. If you upload a 50-page textbook and ask for 10 flashcards, the app will find the 10 most distinct concepts and make one flashcard for each.
5. **Add Instructions (Optional):** You can give the AI a system prompt to guide the output (e.g., "Focus only on the financial data" or "Write for a beginner audience").
6. **Click Create.**

## The Processing Pipeline

Once you click Create, a progress bar shows the pipeline in real-time:

* **Chunking:** Splitting the files.
* **Embedding:** Calculating the vectors.
* **Clustering:** Running K-Means.
* **Generating:** Calling the LLM for each cluster.

You can click **Stop** at any time. If you stop during the "Generating" phase, the clusters that finished are kept.

## Reviewing the Output

When the process finishes, you are presented with the generated outputs in the workspace.

* **Accept:** If you like the output, click **Save Note** to add it to the Knowledge Base (you can assign it to a Project).
* **Reject:** If the output is bad, click **Delete**.
* **Re-synthesize:** You can click **Resynthesize** to run the same inputs again with different instructions or a different output type.

### Exporting

While reviewing the outputs, you can click **Export** to save the file directly to your computer:

* Reports: Markdown (`.md`)
* Slides: Markdown (`.md`)
* Flashcards: CSV (`.csv`)
* Quiz: Markdown (`.md`)
* Audio: MP3 (`.mp3`)
* Dataset: CSV (`.csv`) or JSON (`.json`)

## History

The History panel on the left lists your past synthesis jobs. Clicking one reloads the outputs so you can save them, re-synthesize them, or delete the job entirely.

## Tips for Best Results

* **More input is better:** K-Means works best when it has a lot of text to analyze. Synthesizing a single 1-page PDF is less useful than synthesizing 5 related articles.
* **K should match your goal:** If you want a high-level executive summary, set K to 1 or 2. If you want a study guide with 20 flashcards, set K to 20.
* **The AI needs a key:** Like Research and Skills, Synthesize requires a configured LLM provider in Settings
