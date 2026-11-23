# Commit messages

The sparkle button beside the commit description writes a message from the diff of the **ticked** files. Nothing is committed by generating.

```text
 ✦ click
   │ Workspace::generate_commit_message  (commit.rs)
   ▼
 background thread
   ├─ vekro_git::diff_files(root, ticked_files)         build the diff
   ├─ diff ≤ token limit?  ── yes ─► one request
   │                       └─ no  ─► split per file  → summarize each → merge in a last request
   ▼
 vekro_opencommit::generate(&config, &instructions, &diff)  →  Message { summary, description }
   ▼
 fills the Summary and Description fields (only if the project is still the same one)
```

## Crate layout

| File | Role |
| --- | --- |
| `config.rs` | `Config`, the `~/.opencommit` file; `DEFAULT_MODEL`, `MODELS`, `TOKEN_LIMITS`, `LANGUAGES` |
| `prompt.rs` | `system_prompt`, `estimate_tokens`, `split_diff`, `parse_message` |
| `client.rs` | `generate`: the chat API request |
| `apple.rs` | the on-device Apple Intelligence helper client |

## Configuration

- Settings live in the user's **`~/.opencommit`**, shared with the `oco` command. Unknown lines are kept and the file is mode `0600`. The
  `VEKRO_OPENCOMMIT_CONFIG` variable names another file (for scratch runs).
- The **commit instructions** are in `preferences.json` (`commit_instructions`: `None` is the built-in text, `Some("")` none at all).
- Vekro's `VEKRO_COMMIT_PROVIDER` key selects the optional **Apple Intelligence** provider and leaves the `oco` provider settings available to the CLI.
- **Push after commit**: the commit button runs `git push` right after committing.

## Apple Intelligence

The arm64 app ships a signed Swift helper in `Contents/MacOS`. It calls the on-device Foundation Models framework on macOS 26 or later and
exchanges **JSON over stdin and text over stdout**. It uses a smaller diff budget and merges draft messages in batches. The app reports
availability errors and keeps requests with the selected provider.
