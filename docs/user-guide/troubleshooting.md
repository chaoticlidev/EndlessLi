# Troubleshooting

This guide covers the failures you are most likely to hit: the Python sidecar dropping out, a provider key that stops working, and a script that runs too long. The rest of the page collects the smaller problems that come up along the way.

## Search Fails and Hangs on First Use (Windows Firewall Issue)

Windows Firewall sometimes silently blocks EndlessLi's background research engine. If searches fail immediately:

1. Open **Windows Security** -> **Firewall & network protection**.
2. Click **Allow an app through firewall** -> **Change settings**.
3. Click **Allow another app...** and browse to your EndlessLi install folder.
4. Select **`endlessli-sidecar.exe`** (not the main app).
5. Check both **Private** and **Public**, then click **Add**.

## The Python Sidecar Disconnects

The sidecar is the separate Python process that does web search, page clipping, PDF text extraction, and local embeddings. The app starts it lazily, the first time something needs it, and restarts it if it dies.

### Symptoms

- Research fails immediately with a message about the sidecar.
- The Settings tab shows a sidecar status other than `Running`.
- A PDF import creates the item but has no extracted text.
- The Embedding section says the backend is not available.

### What the App Does About It

On a transport failure the manager drops every in-flight request, counts the crash, and restarts the child on the next call. The status moves through `Restarting` and back to `Running`.

After multiple crashes inside a short window, the manager stops trying and reports:

```text
sidecar disabled after repeated crashes; restart the app to retry
```

At that point the rest of the application still works. Notes, folders, the library, search, and tags do not need the sidecar. What stops working is everything that reaches outside: research, clipping, PDF extraction, and embedding.

### Fixing It

**Restart the app.** This is the first thing to try and it clears the crash counter.

**Check the environment in a development build.** The sidecar runs from `sidecar/.venv`. If that folder is missing, or the requirements were never installed, the sidecar cannot start:

```powershell
# Windows
Test-Path sidecar\.venv\Scripts\python.exe
```

If it is missing, create it:

```powershell
python -m venv sidecar/.venv
sidecar\.venv\Scripts\python.exe -m pip install -r sidecar\requirements.txt
```

**Read the error.** The sidecar writes its diagnostics to stderr, and the app logs every line at warning level. Run `pnpm tauri dev` from a terminal and the reason appears there.

**Check the interpreter.** The sidecar needs Python 3.11 or newer. A development build uses the sidecar environment if it is there, and falls back to `python` or `python3` from `PATH`.

**In a packaged build**, the sidecar has to be a binary beside the executable, named with the target triple. A build without it starts and runs, but every sidecar-backed feature fails.

### A Call That Hangs

A sidecar call is given 90 seconds before it is reported as a timeout, and a health check is given 10 seconds. A timeout reads as:

```text
sidecar <method> timed out
```

For `search.query` this usually means the search provider is slow or unreachable. For `clip.url` it means the page did not answer.

### The Sidecar Process Won't Die (Zombie Processes)

If you force-close the app or it crashes, the Python sidecar process might keep running in the background. This can cause the next launch to fail because the new sidecar can't bind to the same resources.

**How to check:**

1. Open Windows Task Manager.
2. Search for `endlessli-sidecar.exe`.
3. If it is there, right-click and **End Task**.

The app now includes a kill switch on the main window close event, but a hard crash can bypass it. If you see multiple sidecar processes in Task Manager, kill them all and restart the app.

## An API Key Stops Working

### Symptoms

- **Test** in Settings reports a failure.
- Research stops with a message about the provider.
- Settings shows an amber warning above the key list instead of the usual status.

### Checking the Key First

Before you re-enter a key, look at what the operating system thinks it holds. On Windows:

```powershell
cmdkey /list
```

Look for the entry named `endlessli` in the list. On macOS, open Keychain Access and search for `endlessli`.

This tells you which of two very different problems you have:

| The Keychain Entry | What It Means |
| --- | --- |
| Present, and the app says it is missing | A read problem in the app. |
| Absent | Something outside the app removed it. |

Re-adding the key is the fix for the second case and does not help with the first.

### Common Causes

| Cause | How to Tell | Fix |
| --- | --- | --- |
| The key was revoked or expired | The provider's own dashboard | Issue a new key and store it again |
| The wrong base URL | The request fails immediately | Check the **Custom endpoint** field. An OpenAI-compatible server needs its own base URL here. |
| The wrong model name | A 404 or a "model not found" error | Leave the model field blank to use the provider default, or correct the name |
| The budget is exhausted | The message names the daily or monthly limit | Raise the limit in Settings, or wait for the window to roll |
| The keychain is unavailable | The key cannot be stored at all | See the keychain section in [Getting Started](./getting-started.md) |
| The provider account has no credit | The provider rejects the request | Add credit at the provider |

### Storing a Key Again

Open Settings, find the provider, paste the key into its field, and save. Then click **Test**.

The key goes into the keychain and is never shown again, so the field is empty after a save. That is the expected behaviour, not a failure.

### The Key Seems to Have Vanished

If a key disappears from one session to the next, check the keychain first with `cmdkey /list`, as above. If the entry is there, the app did not read it, and the re-add is not the fix. If the entry is gone, something outside the app removed it.

Startup diagnostics help here. The app logs the provider name, whether a model is set, and whether a key was found, and never any key material. Run the app from a terminal to see the line.

## A Script Times Out

### Symptoms

The run panel shows the script as timed out, with no exit code.

### Why It Happens

A script is killed when it runs past its timeout. The default is 60 seconds and the range is 1 to 600 seconds. A killed script has no exit code, because it never got to exit.

### Fixing It

**Raise the timeout.** The **Timeout** field next to **Run script** takes a number of seconds, up to 600.

**Look at stdout first.** A script that prints as it goes tells you where it stopped. The output up to the kill is kept.

**Look for the obvious causes.**

| Cause | How to Spot It |
| --- | --- |
| A network call that never returns | The script tries to open a socket. With **Allow network** off, it fails fast with a permission error instead of hanging. With it on, a blocked connection can sit until the timeout. |
| A very large input | A big text or a large CSV takes longer to process. |
| An accidental infinite loop | The script never reaches its last print. |
| A missing dependency | The script dies on the import with an error in stderr. |

**Check stderr.** The run panel shows stderr separately from stdout. A traceback there tells you exactly where the script stopped.

**Run the same thing by hand.** The parameters are written to a JSON file and the path is in the `ENDLESSLI_INPUT` environment variable. If you can reproduce the hang outside the app, the problem is in the script.

## Smaller Problems

### Auto-Linking Is Off, or the Coverage Bar Does Not Move

The Embedding section in Settings is the place to look.

- **"The backend is not installed"** means the embedding extra is not in the environment the app is actually using. Install it and restart the app:

  ```powershell
  sidecar\.venv\Scripts\python.exe -m pip install -r sidecar\requirements-embeddings.txt
  ```

  Then click **Re-embed missing notes**.

- **Coverage stops partway** means the worker was interrupted. Click **Re-embed missing notes** to queue the rest. The coverage bar refreshes while notes remain.

- **Coverage is at 100 percent and no suggestions appear** means the threshold is too high for the model. Lower **Auto-link threshold** to about 0.5 and save the next note. See [Linking](./linking.md).

### The Embedding Section Does Not Appear

The embedding backend is optional and is installed separately. Until it is present, the section is not available and auto-linking stays off. The rest of the app is unaffected.

### A Research Run Fails Right Away with a Keychain Message

The provider and the model are resolved before the session is created, so a missing key fails cleanly with no session left behind. Add the key in Settings and click **Test**.

### The Daily or Monthly Token Budget Is Exhausted

The message names the limit and how much of it is used:

```text
daily token budget exceeded (812000/1000000 used); raise it in Settings or wait for the window to roll
```

The limit is on total tokens, counting both directions, over a rolling 24-hour or 30-day window. Raise the limit in Settings, or wait for the window to roll over.

### A Research Session Shows "Failed" but You Did Not Cancel It

A session that was running when the app closed has no process behind it. When you open it, the app marks it as failed so it cannot show as running forever. This is cleanup, not a new failure.

### Search Does Not Find a Note You Know Exists

Full-text search matches titles and content. Two cases take a note out of the index:

- **The note is in an encrypted project.** Encrypted notes are stored as ciphertext and are deliberately excluded from the search index, from embeddings, and from wikilink title resolution. This is what stops plaintext leaking through the index. See [Encryption](../encryption.md).
- **The search ran before the note was saved.** The index updates on save.

### A Note in an Encrypted Project Cannot Be Read

The message reads:

```text
decrypt note failed (wrong key or corrupted data)
```

The keychain entry for that project is missing or does not match. The keychain account is named `project-key.<project-id>`, where the project id is the identifier shown in the project list.

There is no recovery key. If the keychain entry is gone, the notes cannot be recovered.

### A PDF Imports without Text

PDF text extraction goes through the sidecar. If the sidecar is missing or unavailable, the import still succeeds and creates the item, but there is no extracted text, so the item is not searchable by content. Fix the sidecar and re-import.

### A Script Cannot Open a Socket

The message from the script is:

```text
Network access is disabled for this script. Enable 'Allow network' in the
Scripts run panel to permit outbound connections.
```

The guard blocks ordinary socket use. It is not an operating-system sandbox, so a script that bypasses the ordinary path is not stopped. See [Skills and Scripts](./skills-and-scripts.md).

### The Scripts Tab Warns That No Interpreter Was Found

Scripts need a Python interpreter. In a development build the sidecar environment is used. In a packaged build the frozen sidecar is not an interpreter, so Python has to be installed and on `PATH`.

### A Skill Returns Nothing

A run with no output is reported as an error:

```text
the model returned no text (it may have spent its whole output budget on
internal reasoning)
```

Some reasoning models consume the entire output budget internally. Use a different model, or narrow the skill.

### A Skill or Script Fails to Save

The validation messages name the problem:

| Message | Cause |
| --- | --- |
| The frontmatter never closes | The `---` block is missing its closing line |
| The name is empty | The `name` field is missing or blank |
| The body is empty | There is no text under the frontmatter |
| A parameter name is repeated | Two parameters share a name |
| The module docstring is missing | The script does not start with a docstring |

Nothing is written when validation fails. Fix the document and save again.

### The App Is Slow to Start on the First Run

The first Rust build compiles DuckDB from source, and that is a long build. Later runs reuse the cache. The database itself opens in under a second on a 10,000-note database. See [Performance](../performance.md).

### Where to Look for Logs

Run the app from a terminal and the logs appear there. The same records are in `logs.db` in the app data directory, in the `logs` table, and you can query them with any SQLite client. Logs are pruned on startup to the most recent 14 days and 5,000 rows, and they never contain key material.

Set `RUST_LOG=debug` before starting the app for verbose output.
