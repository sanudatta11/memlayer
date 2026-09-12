# Design: mobile brand, `.mem` archive, LoCoMo accuracy, Decide

**Date:** 2026-09-12
**Status:** locked for implementation planning
**Companion plan:** `docs/superpowers/plans/2026-09-12-mobile-mem-decide-locomo.md`

This spec covers seven workstreams. They SHOULD land as separate PRs
in rollout order so each is reviewable. Decide consumes conflict
relations; shared Postgres is independent of SQLite file layout.

Naming constraint (global): do not use the word that names any third-party
memory product in code, comments, prompts, docs, CLI help, or commit
messages. Call the new capability **Decide**. Call the conflict LLM the
**resolution judge**. Call the archive a **memlayer archive** (`.mem`).

---

## 1. Problem

1. **Mobile docs header.** On narrow viewports the Starlight site title
   `memlayer` is clipped (`memlaye`) because `.site-title` overflows under
   the search control.
2. **Export/import.** `memlayer sync` only has `status`. Git-chunk JSONL+zstd
   exists in `memlayer-sync` for repo-adjacent sync. JSON/Markdown RPCs are
   unimplemented. There is no user-facing, memlayer-native backup format.
3. **LoCoMo.** The eval crate already has hybrid + facts + entity-walk +
   rerank, but `eval run` defaults to BM25, and `memlayer eval` returns a
   **synthetic** scorecard (it never calls `runner::run`). Published numbers
   therefore do not reflect the retrieval stack.
4. **Decide / conflicts.** Conflict classification exists (`ConflictClassifier`)
   but is **off by default**. `ConflictsWith` currently **supersedes** (soft-
   deletes) the old row. There is no “analyze memories and produce a
   decision” RPC/CLI/MCP tool. Conflicting pairs are not auto-queued for
   resolution.
5. **Agent retrieval is BM25.** Hybrid, facts, recency decay, and
   entity-walk exist in eval but not on the daemon path MCP/hooks use.
   `Context` with a query ignores the query unless mode is hybrid.
6. **Install does not configure memory.** Skills/MCP are written;
   `~/.memlayer/config.toml` is not created or merged.
7. **Single-device files only.** Storage is SQLite per project. TCP mode
   is a gRPC front door to the same files, not a shared brain. There is
   no Postgres/MySQL backend.

---

## 2. Non-goals

- Full disk encryption of `~/.memlayer/` databases.
- Mandatory encryption (default export stays obfuscated-only).
- Replacing git-chunk sync; `.mem` is a portable snapshot, not incremental
  git sync.
- Reimplementing graphify / AST extraction.
- Promising a specific LoCoMo percentage without a measured run.
- MySQL / MariaDB dialect (v1 team store is Postgres + pgvector only).

---

## 3. Workstream A — Mobile site title

**Symptom:** `website` Starlight header at ~390px width clips the last
glyph of `memlayer`.

**Cause:** Starlight’s `.site-title` uses `overflow: hidden` + nowrap, and
the search button sits in the same flex row. The brand shrinks first.

**Fix (chosen):** CSS-only in `website/src/styles/custom.css`.

- At `max-width: 50rem`, set `.site-title` to `flex-shrink: 0`,
  `overflow: visible`, slightly smaller type (`1rem`).
- Hide `.social-icons` on that breakpoint so GitHub does not steal width
  before the brand.
- Ensure the title wrapper cannot ellipsize (`text-overflow: clip` is
  fine; do not use ellipsis on the brand).

**Rejected:** shortening the title to `ml`, swapping to a logo-only header
(brand regression), or custom Starlight header component (too much surface).

**Verify:** render `/docs/agents/` at 390×844 and 768×1024; the eight
letters `memlayer` must be fully visible.

---

## 4. Workstream B — `.mem` archive format

### 4.1 Why a new format

JSON/Markdown export is readable in any editor. The requirement is a
**memlayer-native, compressed, non-legible** portable file. Git-chunk
`.jsonl.zst` is still plaintext JSON after `zstd -d`.

### 4.2 Container (v1)

Always **serialize → compress → wrap**. Never encrypt or obfuscate before
zstd (ciphertext does not shrink).

**Inner payload (size):** MessagePack (`rmp-serde`), compact, with
`skip_serializing_if = "Vec::is_empty"` on list fields. No embeddings
(re-queued on import). Soft-deleted observations stay in the snapshot.

Logical shape (shown as JSON for reading; on disk this is msgpack):

```json
{
  "format": "memlayer.archive",
  "archive_version": 1,
  "schema_version": 8,
  "exported_at": "2026-09-12T00:00:00Z",
  "project": "my-app",
  "observations": [],
  "sessions": [],
  "prompts": [],
  "facts": [],
  "relations": []
}
```

**Compression:** zstd **level 19** (archive path, not the save hot path).
Pin `zstd::encode_all(bytes, 19)`.

Little-endian header (88 bytes):

| Offset | Size | Field |
|--------|------|--------|
| 0 | 4 | Magic `MLYR` (`0x4D 0x4C 0x59 0x52`) |
| 4 | 2 | Format version `u16` = `1` |
| 6 | 1 | Flags. Bit 0 = **encrypted** (seed phrase). Bit 1 = embeddings present (v1 = 0). |
| 7 | 1 | Reserved `0` |
| 8 | 8 | Uncompressed inner (msgpack) length `u64` |
| 16 | 32 | SHA-256 of uncompressed inner |
| 48 | 16 | Salt / XOR nonce (random) |
| 64 | 24 | XChaCha20-Poly1305 nonce (random; ignored when bit 0 is clear) |
| 88 | N | Body (see below) |

**Default wrap (flag bit 0 = 0, not secret):** XOR-obfuscate the zstd
frame with keystream `SHA256("memlayer.mem.v1\0" || salt)` as in the
original plan. Body starts at offset 88. Anyone with this spec can
decode. Docs must not call this encryption.

**Optional seed-phrase wrap (flag bit 0 = 1):**

1. Normalize the seed: Unicode NFC, trim ASCII whitespace / a single
   trailing newline (seed-file friendly). Reject if the normalized
   secret is shorter than 12 Unicode scalars.
2. Derive a 32-byte key with **Argon2id** (RFC 9106), params pinned:
   `m_cost = 65536` (64 MiB), `t_cost = 3`, `p_cost = 1`, version `0x13`,
   salt = header bytes 48–63.
3. Encrypt the **zstd frame** with **XChaCha20-Poly1305** (IETF AEAD,
   24-byte nonce at offset 64). Body = ciphertext || 16-byte tag
   (crate default layout).
4. Zeroize the derived key after use. Never log the seed. Do not XOR
   on top of AEAD (redundant; AEAD output is already non-legible).

Wrong or missing seed → AEAD failure mapped to a clear error:
“invalid seed phrase or archive is not encrypted with a seed”.
Encrypted file + no seed on import → “this archive requires --seed-phrase
or --seed-file”.

Seed is an arbitrary memorable phrase, not a BIP39 wallet mnemonic (no
wordlist check). Recommend six or more words in CLI help.

### 4.3 Size budget

Target: smaller than gzipped JSON of the same rows. Levers locked:

1. MessagePack instead of JSON.
2. Omit empty arrays and omit embedding blobs.
3. zstd-19 after serialize.
4. Unit test: a 50-row fixture’s `.mem` (default wrap) must be **strictly
   smaller** than `zstd -3` of pretty-printed JSON of the same payload.

### 4.3 API

New RPCs (do not overload git-chunk `SyncExport`):

```
rpc ExportMem (ExportMemRequest) returns (ExportMemResponse);
rpc ImportMem (ImportMemRequest) returns (ImportMemResponse);
```

```
message ExportMemRequest {
  string project_name = 1;
  string file = 2;           // destination path; must end with .mem
  // Optional seed phrase. Empty/absent = default obfuscation only.
  optional string seed_phrase = 3;
}
message ExportMemResponse {
  string file = 1;
  int64 observations = 2;
  int64 sessions = 3;
  int64 prompts = 4;
  int64 facts = 5;
  int64 relations = 6;
  int64 bytes = 7;
}
message ImportMemRequest {
  string project_name = 1;
  string file = 2;
  // "merge" (default): upsert by sync_id; "replace": wipe project then insert
  string mode = 3;
  optional string seed_phrase = 4;  // required iff archive flag bit 0 is set
}
message ImportMemResponse {
  int64 observations_imported = 1;
  int64 sessions_imported = 2;
  int64 prompts_imported = 3;
  int64 facts_imported = 4;
  int64 relations_imported = 5;
  int64 skipped = 6;
  bool seed_encrypted = 7;   // peeked from archive flag bit 0
  bool seed_ignored = 8;     // true when a seed was sent but the file was not encrypted
}
```

CLI:

```bash
memlayer mem export --out backup.mem
memlayer mem export --out backup.mem --seed-phrase "twelve or more chars…"
memlayer mem export --out backup.mem --seed-file ./phrase.txt
memlayer mem import backup.mem
memlayer mem import backup.mem --seed-file ./phrase.txt --mode merge
```

`--seed-phrase` and `--seed-file` are mutually exclusive. Prefer
`--seed-file` so the secret does not appear in `ps`. Daemon receives the
seed over the local UDS gRPC channel; handlers must not write it to
`tracing` fields.

**Export UX (required):** after a successful export, the CLI always
prints a short note so the option is discoverable without reading docs.

- **No seed used** (default wrap) — write to **stderr** (TTY and pipes)
  so stdout JSON stays valid:

```
note: archive is not seed-encrypted (obfuscated only).
      encrypt with a seed phrase (same phrase required on import):
        memlayer mem export --out FILE.mem --seed-file ./phrase.txt
        memlayer mem export --out FILE.mem --seed-phrase 'your phrase here'
      keep the phrase; it cannot be recovered.
```

- **Seed used** — stderr:

```
note: archive is seed-encrypted. import needs the same --seed-file or --seed-phrase.
      if you lose the phrase, this file cannot be opened.
```

JSON `--output json` includes `"seed_encrypted": false` and
`"hint": "<first line of the note>"` on the success object. Clap
`--help` for `mem export` must mention both flags in the command
about/long_help (same wording, not only after a run).

**Import UX (required):** the CLI peeks the `.mem` header locally
(`is_encrypted`) *before* the import RPC so messages match the file,
not guesswork.

**Encrypted file + no `--seed-phrase` / `--seed-file` → always a hard
error** (TTY and pipes). Do not prompt. Do not attempt decrypt. Exit
code = usage/invalid argument (same family as other CLI flag errors).
Stderr (exact shape; substitute the user path for `FILE.mem`):

```
error: this archive is seed-encrypted and cannot be imported without the seed phrase.
       pass the same phrase used at export:
         memlayer mem import FILE.mem --seed-file ./phrase.txt
         memlayer mem import FILE.mem --seed-phrase 'your phrase here'
```

JSON `--output json` on this failure:

```json
{"error":"seed_required","message":"this archive is seed-encrypted and cannot be imported without the seed phrase","seed_encrypted":true}
```

The daemon must return the same failure if the CLI skipped the peek
(`decode(..., None)` on an encrypted body → `SyncError::SeedRequired` →
gRPC `InvalidArgument` with that message). Never surface zstd/AEAD
internals (“tag mismatch”, “invalid utf-8”) for this case.

| File | Flags | Behavior |
|---|---|---|
| Encrypted | no seed | **Error above.** No prompt, no partial import. |
| Encrypted | seed present | Import. Wrong seed → “invalid seed phrase or corrupt archive” (do not say which). |
| Not encrypted | no seed | Import. Stderr note that the file is obfuscated only. |
| Not encrypted | seed present | Import anyway. Stderr warn: seed was ignored because the archive is not seed-encrypted. Do not fail (scripts may always pass `--seed-file`). |

Success notes (stderr; JSON gets `seed_encrypted` + `hint`):

- **Encrypted import succeeded:**

```
note: imported a seed-encrypted archive.
      future imports of this file need the same --seed-file or --seed-phrase.
```

- **Unencrypted import succeeded:**

```
note: archive is not seed-encrypted (obfuscated only).
      if you expected encryption, re-export with --seed-file or --seed-phrase.
```

- **Encrypted, missing seed (required error):**

```
error: this archive is seed-encrypted and cannot be imported without the seed phrase.
       pass the same phrase used at export:
         memlayer mem import FILE.mem --seed-file ./phrase.txt
         memlayer mem import FILE.mem --seed-phrase 'your phrase here'
```

`mem import --help` must list both seed flags. Never print the phrase.
Never skip the unencrypted success note when stdout is not a TTY.

Reject paths that do not end in `.mem`. Magic mismatch → clear error
“not a memlayer archive”.

Implementation lives in `crates/memlayer-sync/src/mem_archive.rs` (codec)
and daemon handlers that snapshot via the write thread (`WriteRequest::Custom`).

### 4.4 Approaches considered

| Approach | Pros | Cons |
|---|---|---|
| **A. Default XOR + optional Argon2id/XChaCha20-Poly1305 (chosen)** | Small (msgpack+zstd-19); real secrecy when the user passes a seed; no extra wrap format | Lost seed = unrecoverable archive |
| B. Plain `.jsonl.zst` renamed `.mem` | Already exists | Readable after `zstd -d` |
| C. age(1) encrypted wrapper | Battle-tested CLI | External binary, worse size, not memlayer-native |

---

## 5. Workstream C — Better LoCoMo numbers

### 5.1 Root cause (not “the model is weak”)

The retrieval stack that can score well is already in `memlayer-eval`
(`retrieve_facts`, entity walk, hybrid RRF, optional Haiku rerank,
quality modifiers). Two wiring bugs hide it:

1. `crates/memlayer-eval/src/bin/eval.rs` defaults `--mode` to **Bm25**.
2. `crates/memlayer-cli/src/cmd_eval.rs` builds a `RunConfig` then
   **ignores it** and prints a synthetic `RunReport`. CI scorecards are
   not real LoCoMo.

### 5.2 Changes

1. **`memlayer eval` runs the real harness** when `data/locomo/locomo10.json`
   is present. If the dataset is missing, exit non-zero with download
   instructions — never invent accuracy.
2. **Default retrieval for LoCoMo** = `default_profile(Locomo)` already
   defined: `HybridRerank`, `k=10`, `evidence_window=2`, `rerank=true`.
   Change the eval binary default from BM25 to that profile (flag still
   overrides).
3. **Auto-extract:** if `facts.db` is missing and mode is Hybrid/HybridRerank,
   run `extract_pipeline` before queries (same as `eval extract`). Smoke
   mode (`--limit`/`--smoke`) extracts only windows for ingested rows.
4. **Category routing (eval-only, no daemon schema change):**
   - Temporal questions (`when`, `before`, `after`, dates): boost facts
     with non-empty `temporal`; keep evidence window.
   - Multi-hop (`and`, `both`, `also`): keep entity-walk (already wired
     in `retrieve_facts`).
   - Single-hop: skip rerank when `top2_delta >= 0.15` (already gated).
5. **Judge prompt:** include extracted facts *and* evidence-window
   observation text so the answer LLM is not starved on “who said what”.
6. **Scorecard honesty:** persist real `accuracy_pct`, retrieval p50, and
   whether extract/rerank ran. Compare against Mem0 published 91.6% as a
   **reference column only**, not a fake self-score.

**Target:** measured LoCoMo overall strictly above the current BM25-only
path. Do not hard-code a claimed percentage in docs until a full run
exists. Stretch goal vs Mem0 New is post-measurement.

### 5.3 Approaches considered

| Approach | Pros | Cons |
|---|---|---|
| **A. Wire existing hybrid+facts+rerank and stop synthesizing (chosen)** | Uses code we already have; highest ROI | Needs dataset + Claude CLI in CI for full numbers |
| B. New embedder (BGE-base / larger) | Maybe +1–3 pts | Binary size, latency, new model download |
| C. Fine-tune a local reranker | No API | Training data, infra; not this spec |

CI: keep `--smoke` on a tiny fixture so GitHub Actions does not need
the full LoCoMo dump or paid tokens. Full runs stay `eval run --benchmark locomo`.

---

## 6. Workstream D — Decide + auto resolution judge

### 6.1 Product behavior

**Decide** answers a question using stored memory, surfaces disagreements,
and either:

- returns a recommendation with citations, or
- records a `type=resolution` observation when the model can settle a
  conflict.

It is **not** a general chatbot. It only reasons over retrieved
observations + facts.

### 6.2 Conflict on save (change from today)

Today (`write.rs` `should_supersede`):

| Verdict | Effect |
|---|---|
| Supersedes | soft-delete old |
| ConflictsWith | soft-delete old (**wrong for “keep until decided”**) |
| Compatible / NotConflict | keep both |

**New:**

| Verdict | Effect |
|---|---|
| Supersedes | soft-delete old; relation `supersedes` |
| ConflictsWith | **keep both**; relation `conflicts_with`; enqueue `ResolveJob` |
| Compatible / NotConflict | keep both; existing relations |

Default `conflict.enabled = true`. If the judge errors or Claude is
missing, keep the BM25 heuristic **but** if the title/type/scope match
is strong, still enqueue `ResolveJob` instead of blindly deleting when
the heuristic is the only signal — wait: that would change all saves.

Safer fallback: on judge **error**, keep current heuristic supersession
(unconditional) so offline daemons do not accumulate duplicates. On
successful `ConflictsWith`, never delete. Document that
`conflict.enabled=false` restores pre-spec heuristic.

Candidate finding stays FTS5 title match (existing). Follow-up: dense
neighbor scan is out of this spec.

### 6.3 Resolve worker

New daemon pool `resolve_worker` (same pattern as extract: `try_queue`,
never block save).

Job payload: `{project, new_id, old_id}`.

Worker:

1. Load both observations + overlapping facts.
2. Prompt (Haiku, `conflict.timeout_secs` or 15s for resolve): classify
   `keep_new`, `keep_old`, `keep_both`, `synthesize`.
3. Apply:
   - `keep_new` → supersede old
   - `keep_old` → supersede new (rare; only if new is a mistaken duplicate)
   - `keep_both` → leave rows; relation stays
   - `synthesize` → `SaveObservation` with `type=resolution`,
     `topic_key` shared, content = chosen statement + citations;
     supersede neither until the agent confirms (v1: do **not** auto-
     delete on synthesize; the resolution row is the new source of
     truth for Decide)
4. Write relation `resolved_by` from both ids → resolution id when
   synthesize/keep_* completes.

`try_queue` drop on full → log warning; `memlayer doctor` can list
unresolved `conflicts_with` with no `resolved_by`.

### 6.4 Decide RPC

```
rpc Decide (DecideRequest) returns (DecideResponse);

message DecideRequest {
  string project_name = 1;
  string question = 2;
  int32 limit = 3;             // retrieval k, default 12, max 30
  optional string mode = 4;    // bm25 | hybrid (default hybrid)
}

message DecideEvidence {
  int64 observation_id = 1;
  string title = 2;
  string content = 3;
  string role = 4;             // "supports" | "opposes" | "context"
}

message DecideConflict {
  int64 a_id = 1;
  int64 b_id = 2;
  string relation = 3;
  string status = 4;           // "open" | "resolved"
}

message DecideResponse {
  string recommendation = 1;
  string rationale = 2;
  float confidence = 3;        // 0..1
  repeated DecideEvidence evidence = 4;
  repeated DecideConflict conflicts = 5;
  optional int64 resolution_observation_id = 6;  // if a resolution row was written
  bool wrote_resolution = 7;
}
```

Pipeline:

1. Hybrid search + facts (daemon retrieval, same as Context).
2. Load `conflicts_with` edges among hit ids (and one-hop neighbors).
3. If any **open** conflicts, run resolution judge on those pairs
   **synchronously for this RPC** (user asked for auto-trigger). Save
   path still uses the async worker; Decide does not wait for the
   worker — it judges live so the answer is consistent.
4. LLM synthesis over evidence + conflict verdicts → structured JSON
   parsed into `DecideResponse`.
5. If the model returns `should_record=true` and confidence ≥ 0.7,
   save a `type=resolution` observation (idempotent topic_key
   `decision/<normalized-question-hash>`).

CLI: `memlayer decide "Should we keep SQLite or move to Postgres?"`
MCP: `memory_decide` with `{ question, project?, limit? }`.

### 6.5 Approaches considered

| Approach | Pros | Cons |
|---|---|---|
| **A. Keep ConflictsWith rows + async resolve + sync Decide (chosen)** | Matches “auto trigger judge”; save stays fast | Two paths (worker + Decide) must share prompt/parser |
| B. Always block save on LLM judge + resolution | Simpler | Save latency 5–15s; violates worker-pool design |
| C. Decide as prompt-only, no new observation type | Less schema | Agents cannot retrieve prior decisions as memory |

Share `build_conflict_prompt` / parse helpers between
`conflict_judge.rs` and `resolve_worker.rs`. Do not duplicate prompts.

---

## 7. File map (all workstreams)

| Path | Responsibility |
|---|---|
| `website/src/styles/custom.css` | Mobile brand overflow |
| `crates/memlayer-sync/src/mem_archive.rs` | `.mem` encode/decode |
| `crates/memlayer-sync/src/lib.rs` | Export module |
| `crates/memlayer-daemon/src/mem_export.rs` | Snapshot + write file |
| `crates/memlayer-daemon/src/mem_import.rs` | Decode + write-thread upsert |
| `proto/memlayer.proto` | ExportMem, ImportMem, Decide |
| `crates/memlayer-cli/src/cmd_mem.rs` | `mem export` / `mem import` |
| `crates/memlayer-cli/src/cmd_decide.rs` | `decide` |
| `crates/memlayer-cli/src/cmd_eval.rs` | Real eval runner |
| `crates/memlayer-eval/src/bin/eval.rs` | Default LoCoMo profile |
| `crates/memlayer-daemon/src/resolve_worker.rs` | Async conflict resolution |
| `crates/memlayer-daemon/src/decide.rs` | Decide handler |
| `crates/memlayer-storage/src/write.rs` | ConflictsWith keeps both |
| `crates/memlayer-core/src/config.rs` | `search`, `conflict`, `storage` config |
| `crates/memlayer-cli/src/cmd_skill.rs` | `ensure_memlayer_home` on install |
| `crates/memlayer-retrieval/src/facts_fuse.rs` | Port eval fact+RRF into daemon |
| `crates/memlayer-daemon/src/service.rs` | Hybrid default, context+query, fact search |
| `crates/memlayer-storage/src/store.rs` | `MemoryStore` trait |
| `crates/memlayer-storage/src/postgres.rs` | Postgres + pgvector backend |
| `migrations/postgres/*.sql` | PG schema |
| `website/src/content/docs/docs/config.md` | storage.backend + DATABASE_URL |
| `crates/memlayer-mcp/src/server.rs` | `memory_decide` |
| `website/src/content/docs/docs/commands.md` | Document mem + decide + shared DB |
| `docs/ROADMAP.md`, `crates/memlayer-cli/src/cmd_session.rs` | Strip forbidden product name |

Schema: **no new migration** if `type=resolution` is just an observation
type string and `resolved_by` is a `observation_relations.relation_type`.
V8 already allows arbitrary `relation_type`.

---

## 8. Workstream E — Search, save, and LoCoMo-quality retrieval in the daemon

Code audit (current `main`): the eval crate already has hybrid + facts +
entity-walk + recency decay + contradiction penalties. The **daemon
path agents actually use** does not.

### 8.1 Gaps that keep us behind

| Gap | File | Effect |
|---|---|---|
| Search/context/MCP/hooks default **bm25** | `cli.rs`, `mcp/server.rs`, `cmd_hook.rs`, `HybridMode::parse_wire` | Agents never get dense recall unless they pass `--mode hybrid` |
| `Context` with a query in BM25 mode **ignores the query** | `service.rs` context handler | `memory_context` / `obs context --query` returns recency, not answers |
| Facts never used in daemon search | `storage/facts.rs` `search_facts` unused | LoCoMo temporal/multi-hop techniques stay eval-only |
| No recency decay in daemon | eval `scoring.rs` only | Stale memories rank equal to new ones |
| `extract.enabled` / `conflict.enabled` default false | `config.rs` | No atomic facts, heuristic supersession |
| `memory_add` never sets `topic_key` | `mcp/server.rs` | No topic supersession from MCP writes |
| Embed/extract `try_queue` drop is silent to the user | `embed_worker.rs` | Memories saved without vectors |
| All-projects hybrid skips rerank | `service.rs` early return | `--all-projects` weaker than single-project |
| `memlayer eval` synthetic report | `cmd_eval.rs` | Published numbers are fake |

### 8.2 Production retrieval (chosen)

1. Add `[search]` to `MemlayerConfig`:
   - `mode = "hybrid"` (default; `bm25` still valid)
   - `rerank = false` (default; cost). LoCoMo eval profile may still rerank.
   - `decay_lambda = 0.005` (same as eval)
2. Wire parse: empty `mode` on Search/Context/MCP/hooks uses
   `cfg.search.mode`, not hardcoded bm25.
3. **Context:** if `query` is non-empty, always retrieve (hybrid or
   BM25 per config). Recency briefing only when query is empty.
4. **Fact-aware search:** if the project has facts, run `search_facts`
   (BM25 + dense if present), map fact hits → parent `obs_id`, RRF-fuse
   with observation hybrid lists (eval `retrieve_facts` algorithm,
   ported into `memlayer-retrieval`, **called by the daemon**).
5. Apply eval `apply_quality_modifiers` (salience, decay, contradiction)
   when facts exist.
6. **Save:** if `topic_key` empty, call existing `SuggestTopicKey` logic
   inline (no extra RPC) before insert. MCP `memory_add` gets the same.
7. When embed/extract queue drops, set `SaveObservationResponse` warning
   string `embed_dropped` / `extract_dropped` (already has similar
   fields pattern via `similar_observations`).
8. Cross-project hybrid: run rerank when requested (remove early-return
   skip).

LoCoMo: Task 5 still wires the eval harness. This workstream makes
**production search match the eval stack**, which is what actually
beats other memory products in the agent loop.

### 8.3 Approaches considered

| Approach | Pros | Cons |
|---|---|---|
| **A. Port eval retrieve_facts into daemon + hybrid default (chosen)** | Same code that can score LoCoMo; agents benefit immediately | Extract must be on for full lift |
| B. Keep BM25 default, document `--mode hybrid` | No surprise latency | Agents never flip the flag (today’s failure) |
| C. New embedder (larger than BGE-small) | Maybe +1–3 pts | Binary size; do after A is measured |

---

## 9. Workstream F — `memlayer install` writes local config

Today install updates skills, hooks, MCP JSON. It **never** creates or
merges `~/.memlayer/config.toml`. New agents therefore stay on BM25,
extract off, conflict off.

**Required:** at the start of `cmd_skill::dispatch`, call
`ensure_memlayer_home()`:

1. `paths::ensure_dirs` (data dir, projects, socket parent).
2. If `~/.memlayer/config.toml` **missing**, write:

```toml
[search]
mode = "hybrid"
rerank = false

[extract]
enabled = true
model = "haiku"

[conflict]
enabled = true
model = "haiku"

[embed]
workers = 2

[storage]
backend = "sqlite"
```

3. If the file **exists**, deep-merge **only keys that are absent**.
   Never overwrite a user’s `storage.postgres_url` or a custom
   `search.mode`. If `extract.enabled` is missing, set `true`.
4. Print a summary: backend, search.mode, extract, conflict.
5. If `extract.enabled` and `claude` is not on PATH, warn (do not
   disable).
6. Restart hint: “restart the daemon (`memlayer daemon restart`) so
   workers pick up config.”
7. MCP upsert behavior stays (already merge-safe).

Unit test: temp HOME, missing file → written; existing file with
`[search] mode = "bm25"` stays bm25; missing `[conflict]` gets enabled.

---

## 10. Workstream G — Shared team database (Postgres)

**Default remains SQLite files** (`~/.memlayer/projects/<id>.db` +
`global.sqlite`). Fastest for a single device: no network, FTS5,
sqlite-vec.

**Shared brain across devices:** a hosted SQL database all daemons
connect to. Multiple laptops / CI agents see the same observations.

### 10.1 Engine choice

| Engine | FTS | Vectors | Hosted | Verdict |
|---|---|---|---|---|
| **PostgreSQL + pgvector** | `tsvector` + GIN | first-class `vector(384)` IVFFlat/HNSW | RDS, Cloud SQL, Neon, Supabase, Crunchy, self-host | **Chosen** |
| MySQL 8 / MariaDB | FULLTEXT (boolean, weaker ranking) | HeatWave / experimental; not portable | RDS, PlanetScale | Rejected for ANN + ranking |
| libSQL / Turso | FTS5 | sqlite-vec over HTTP is awkward | Turso | Follow-up if we need SQLite-shaped edge replicas |
| SQLite on NFS | FTS5 | sqlite-vec | none | Unsafe writers; not a team store |

Postgres is the fastest *hosted* option for this mix (concurrent
writers, GIN FTS, HNSW ANN, `LISTEN/NOTIFY`). SQLite stays fastest
*local*.

### 10.2 Config

```toml
[storage]
backend = "sqlite"          # default
# backend = "postgres"
# url = "postgres://user:pass@host:5432/memlayer"
```

Env (highest precedence): `MEMLAYER_STORAGE_BACKEND`,
`MEMLAYER_DATABASE_URL` (standard name so hosted platforms inject it).

When `backend = "postgres"`, ignore per-project file paths for data;
`project_name` is a column. Local UDS daemon still runs on each device
(embed/extract/rerank stay local). Only the system of record moves.

### 10.3 Schema (logical)

Same entities as SQLite V8: `observations`, `sessions`, `user_prompts`,
`facts`, `observation_relations`, `observation_embeddings` (pgvector),
`sync_id UUID UNIQUE`.

Search: generated `tsvector` on title+content+topic_key; GIN index.
Dense: `embedding vector(384)` + HNSW. Fact search: separate
`facts_tsv` + `facts.embedding`.

Migrations live in `migrations/postgres/V{N}__name.sql`, applied with
`sqlx` (async, matches the daemon). Do **not** run SQLite refinery
against Postgres.

### 10.4 Concurrency and “same brain”

- Each device: `memlayer` CLI → local daemon → **Postgres**.
- Saves use `INSERT ... ON CONFLICT (sync_id)` so two devices cannot
  fork the same observation.
- Conflict judge + Decide run on whichever daemon handled the save;
  relations are visible to all.
- Optional `LISTEN memlayer_invalidate` after writes so other daemons
  can drop project caches (LRU). v1 can skip NOTIFY and rely on
  read-after-write to PG.
- Auth to PG is the database URL (TLS). Memlayer TCP tokens remain for
  exposing gRPC; they are **not** a substitute for the shared DB.

### 10.5 Trait (do not fork the daemon)

```rust
#[async_trait]
pub trait MemoryStore: Send + Sync {
    fn backend_kind(&self) -> BackendKind; // Sqlite | Postgres
    async fn save_observation(&self, input: SaveObservationInput) -> Result<Observation>;
    async fn search(&self, q: SearchQuery) -> Result<Vec<Observation>>;
    async fn search_facts(&self, q: SearchQuery) -> Result<Vec<FactHit>>;
    // get/update/delete/list/recent/relations/history — same surface as today
}
```

`SqliteStore` wraps today’s `ProjectRegistry` + write thread.
`PostgresStore` uses `sqlx::PgPool` (max 16 connections per daemon).

CLI: `memlayer doctor` reports backend + ping. `memlayer install`
does not force Postgres. Document hosted setup in `docs/config.md`.

### 10.6 Out of scope for G

- MySQL dialect
- Automatic SQLite→Postgres dump beyond `mem export` / `mem import`
  (import into a postgres-backed daemon **is** supported: decode `.mem`
  then save via the store)
- Multi-master / CRDT. Postgres is the single writer store.

---

## 11. Testing (all workstreams)

- CSS: Playwright optional; `npm run build` required.
- `mem_archive`: round-trip default and seeded; encrypted-without-seed
  is `SeedRequired` with the user-facing error; wrong seed fails;
  truncated/wrong magic fail; inner format not UTF-8-legible; size vs
  JSON+zstd-3.
- Eval: no synthetic accuracy; missing dataset errors.
- Decide: mock classifier; ConflictsWith keeps both rows.
- Search: empty mode → hybrid from config; context+query does not
  return a query-free recent list.
- Install: temp HOME writes config; does not clobber existing mode.
- Postgres: sqlx tests behind `#[cfg(feature = "postgres")]` or skipped
  unless `MEMLAYER_DATABASE_URL` is set; Travis-style skip is OK. Unit
  tests for SQL builders must not require a live server.

---

## 12. Rollout order

1. A — mobile CSS
2. B — `.mem` archive + seed UX
3. E then F — hybrid default + install config (largest agent-visible lift)
4. C — real LoCoMo eval
5. D — Decide + conflict keep-both
6. G — Postgres shared store (own PR; needs sqlx + hosted instance)

---

## 13. Spec self-review

- Placeholders: none.
- Scope: seven workstreams; still one spec. Implement as separate PRs
  per rollout order.
- Ambiguity: default `.mem` obfuscated; seed enables AEAD. Shared DB is
  **Postgres only** in v1. Local default remains SQLite files.
- LoCoMo: no hardcoded % in README until a real run.
- Forbidden third-party memory product names: still banned.
