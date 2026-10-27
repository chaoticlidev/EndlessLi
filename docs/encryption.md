# Encryption

EndlessLi offers two independent layers:

1. **Per-project note encryption** (available in every build): AES-256-GCM
   with a key stored in the OS keychain.
2. **Full-database encryption** (optional build feature): SQLCipher.

They can be used together or separately.

## Per-Project Note Encryption

### Enabling

1. Open the Knowledge Base.
2. Hover a project in the left panel and click the lock icon.
3. Confirm. Every note currently in the project is encrypted in place, the
   project's vectors are deleted, and the project shows a lock indicator.

The project key is generated locally (32 random bytes) and stored in the OS
keychain under `endlessli` / `project-key.<project-id>`. It is never written to
SQLite.

### What Changes

- Note titles and content are stored as
  `enc:v1:<base64(nonce||ciphertext)>`. A fresh nonce is used for every write.
- Reads decrypt transparently in the repository layer; the UI behaves normally.
- New notes, edits, branches, and research/library/skill saves into the project
  are encrypted automatically.
- Moving a note into or out of the project re-encodes it for the destination.

### Limitations (By Design)

- **Search does not cover encrypted notes.** The FTS index holds ciphertext.
- **Auto-linking and embeddings skip encrypted notes.** Embeddings are stored
  in plaintext in the vector table, so computing them would leak content.
- **`[[wikilink]]` title lookup does not match encrypted titles.** The wikilink
  itself still renders, but the note is not resolved by title.
- **Losing the keychain entry loses the notes.** There is no recovery key.
  Back up the OS credential entry alongside the database if you need a
  recoverable backup, or decrypt the project before archiving it.

### Disabling

Click the lock again and confirm. Every note is decrypted in place, the key is
deleted from the keychain, and the notes are queued for re-embedding. If the
key is already missing, decryption fails before anything is written; restore the
keychain entry first.

### Deleting an Encrypted Project

Deleting an encrypted project first decrypts its notes (they move to Inbox) and
then removes the key, so content is not orphaned as unreadable ciphertext.

## Full-Database Encryption (SQLCipher)

This is a compile-time feature and is **off by default**.

### Build

```powershell
cargo build -p endlessli --features sqlcipher
# or for the app bundle
pnpm tauri build --features sqlcipher
```

`el-db/sqlcipher` enables `rusqlite/bundled-sqlcipher`. SQLCipher is compiled
from source; OpenSSL is supplied by the platform, not built by Cargo.

On Windows, install the static OpenSSL package with vcpkg and point Cargo at
the installed tree. If vcpkg is not installed, clone it and run the bootstrap
script first:

```powershell
git clone https://github.com/microsoft/vcpkg.git C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat
C:\vcpkg\vcpkg.exe install openssl:x64-windows-static
$env:OPENSSL_DIR = "C:\vcpkg\installed\x64-windows-static"
$env:OPENSSL_LIB_DIR = "$env:OPENSSL_DIR\lib"
$env:OPENSSL_INCLUDE_DIR = "$env:OPENSSL_DIR\include"
cargo build -p endlessli --features sqlcipher
```

Adjust `OPENSSL_DIR` when vcpkg is installed elsewhere. The
`x64-windows-static` triplet uses the static C runtime; if the linker reports
a `RuntimeLibrary` mismatch (`MT` versus `MD`), install
`openssl:x64-windows-static-md` instead and point the same three variables at
that directory.

On macOS and Linux, install OpenSSL with the system package manager (for
example `brew install openssl@3` or `apt install libssl-dev`) and set
`OPENSSL_DIR` when the headers are not in a default location. The first build
compiles SQLCipher and takes several minutes.

### Providing the Passphrase

At startup the app looks for, in order:

1. `ENDLESSLI_DB_KEY` (development/testing).
2. The OS keychain entry `endlessli` / `database-key`.

If neither exists, the database opens unencrypted (the default). If a key is
present, the app applies `PRAGMA key` before any other statement.

### Moving an Existing Database

A plaintext database cannot be opened with a key. To migrate:

1. Close the app.
2. Use the `sqlcipher` CLI (or `sqlcipher_export`) to create an encrypted copy:

   ```sql
   PRAGMA key = 'your-passphrase';
   ATTACH DATABASE 'endlessli-encrypted.db' AS encrypted KEY 'your-passphrase';
   SELECT sqlcipher_export('encrypted');
   DETACH DATABASE encrypted;
   ```

3. Replace `endlessli.db` with the encrypted copy and delete the `-wal`/`-shm`
   files (or checkpoint the old database first).
4. Store the same passphrase in the keychain as `database-key` (or set
   `ENDLESSLI_DB_KEY` for development).

Losing the passphrase makes the database unrecoverable. There is no recovery
path.

### Verification

- Wrong key or a plaintext database opened with a key: startup fails with a
  database error instead of silently continuing.
- Without the `sqlcipher` feature, supplying a key returns a clear
  "compiled without the sqlcipher feature" error rather than ignoring it.
