# Mobile brand, `.mem` archive, LoCoMo wiring, Decide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the clipped mobile `memlayer` header; add a native compressed `.mem` archive; make LoCoMo eval actually run hybrid+facts retrieval; add Decide plus auto resolution on conflicting memories — without naming any third-party memory product anywhere in the tree.

**Architecture:** Four sequential PRs. CSS-only header fix. Archive codec in `memlayer-sync` (MLYR + zstd + XOR keystream) with daemon snapshot/import. Eval CLI calls `runner::run` and defaults to `default_profile(Locomo)`. Save path keeps `ConflictsWith` rows, queues `resolve_worker`; `Decide` RPC retrieves, judges open conflicts, returns a structured recommendation and may write `type=resolution`.

**Tech Stack:** Starlight CSS, Rust, proto3/tonic, rusqlite, zstd, SHA-256, existing `ClaudeClient` shell-out, rmcp MCP tools.

## Global Constraints

- Never write the substring matching a third-party memory product name in code, comments, prompts, docs, CLI help, or commits. Use **Decide**, **resolution judge**, **memlayer archive**.
- `.mem` v1 is **obfuscation**, not encryption. Do not document it as encrypted.
- Migrations: no new SQL file unless a later task proves V8 `observation_relations.relation_type` is insufficient. Prefer `resolved_by` as a relation string.
- Write-thread: all DB mutations go through `WriteRequest` / `WriteRequest::Custom`.
- Worker pools: `try_queue` drops on full; never block save.
- Config: 3-level TOML merge; re-resolve per task; `conflict.enabled` default becomes `true`.
- Tests: unit tests in crate `#[cfg(test)]`; do not rely on a live daemon for codec/eval-fixture tests.
- Branch names: `cursor/<descriptive>-8379` if this agent implements.

**Spec:** `docs/superpowers/specs/2026-09-12-memory-export-decide-locomo-design.md`

**Suggested PR split:** Tasks 1 | Tasks 2–5 | Tasks 6–7 | Tasks 8–12. Do not merge all four into one review if the diff exceeds ~800 lines of Rust.

---

## File map (locked)

| File | Role |
|---|---|
| `website/src/styles/custom.css` | Mobile header overflow |
| `crates/memlayer-sync/src/mem_archive.rs` | Encode/decode `.mem` |
| `crates/memlayer-sync/src/lib.rs` | `pub mod mem_archive` |
| `crates/memlayer-sync/src/error.rs` | Archive errors |
| `proto/memlayer.proto` | `ExportMem`, `ImportMem`, `Decide` |
| `crates/memlayer-daemon/src/mem_export.rs` | Export handler |
| `crates/memlayer-daemon/src/mem_import.rs` | Import handler |
| `crates/memlayer-daemon/src/decide.rs` | Decide handler |
| `crates/memlayer-daemon/src/resolve_worker.rs` | Async resolve pool |
| `crates/memlayer-daemon/src/service.rs` | RPC wiring |
| `crates/memlayer-daemon/src/lib.rs` | Module decls |
| `crates/memlayer-storage/src/write.rs` | Keep both on ConflictsWith; enqueue hook |
| `crates/memlayer-core/src/config.rs` | Default conflict on |
| `crates/memlayer-cli/src/cli.rs` | `Mem` + `Decide` commands |
| `crates/memlayer-cli/src/cmd_mem.rs` | CLI handlers |
| `crates/memlayer-cli/src/cmd_decide.rs` | CLI handler |
| `crates/memlayer-cli/src/cmd_eval.rs` | Real runner |
| `crates/memlayer-cli/src/main.rs` | Dispatch |
| `crates/memlayer-eval/src/bin/eval.rs` | Default HybridRerank for locomo |
| `crates/memlayer-mcp/src/server.rs` | `memory_decide` |
| `crates/memlayer-mcp/src/tools/mod.rs` | `DecideArgs` |
| `website/src/content/docs/docs/commands.md` | User docs |
| `docs/ROADMAP.md`, `crates/memlayer-cli/src/cmd_session.rs`, `crates/memlayer-cli/src/cli.rs`, `skills/memlayer/references/hooks.md` | Rename comments |

---

### Task 1: Mobile site title fully visible

**Files:**
- Modify: `website/src/styles/custom.css`
- Test: load `website` at 390px width (Playwright if present; otherwise `npx --yes playwright` is out of scope — use a CSS regression comment and `npm run build` in `website/`)

**Interfaces:**
- Consumes: Starlight `header.header`, `.site-title`, `.social-icons`
- Produces: brand string `memlayer` unclipped at ≤50rem

- [ ] **Step 1: Append mobile header rules**

Add at the end of `website/src/styles/custom.css`:

```css
/* Narrow viewports: never clip the brand. Starlight's .site-title
   overflow:hidden + flex-shrink lets search overlap the last glyph. */
@media (max-width: 50rem) {
	header.header {
		padding-inline: 0.75rem;
		gap: 0.35rem;
	}

	header.header .site-title,
	header.header a.site-title {
		flex-shrink: 0;
		overflow: visible;
		min-width: max-content;
		max-width: none;
		font-size: 1rem;
		letter-spacing: -0.03em;
	}

	header.header .site-title span {
		overflow: visible;
		text-overflow: clip;
		white-space: nowrap;
	}

	header.header .social-icons {
		display: none;
	}
}
```

- [ ] **Step 2: Build the site**

```bash
cd website && npm ci && npm run build
```

Expected: exit 0.

- [ ] **Step 3: Commit**

```bash
git add website/src/styles/custom.css
git commit -m "fix(website): keep full memlayer brand visible on mobile"
```

---

### Task 2: `.mem` codec (encode/decode + illegibility test)

**Files:**
- Create: `crates/memlayer-sync/src/mem_archive.rs`
- Modify: `crates/memlayer-sync/src/lib.rs`
- Modify: `crates/memlayer-sync/src/error.rs`

**Interfaces:**
- Consumes: `zstd`, `sha2`, `serde_json`
- Produces:
  - `pub const MAGIC: &[u8; 4] = b"MLYR";`
  - `pub const ARCHIVE_VERSION: u16 = 1;`
  - `pub struct ArchivePayload { ... }`
  - `pub fn encode(payload: &ArchivePayload, nonce: [u8; 16]) -> Result<Vec<u8>>`
  - `pub fn decode(bytes: &[u8]) -> Result<ArchivePayload>`
  - `pub fn keystream_xor(data: &[u8], nonce: &[u8; 16]) -> Vec<u8>`

- [ ] **Step 1: Add errors**

In `crates/memlayer-sync/src/error.rs` add variants:

```rust
#[error("not a memlayer archive (bad magic)")]
NotArchive,

#[error("unsupported memlayer archive version {0}")]
UnsupportedVersion(u16),

#[error("memlayer archive truncated")]
Truncated,

#[error("memlayer archive checksum mismatch")]
ChecksumMismatch,
```

- [ ] **Step 2: Write failing tests in `mem_archive.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ArchivePayload {
        ArchivePayload {
            format: "memlayer.archive".into(),
            archive_version: 1,
            schema_version: 8,
            exported_at: "2026-09-12T00:00:00Z".into(),
            project: "demo".into(),
            observations: vec![],
            sessions: vec![],
            prompts: vec![],
            facts: vec![],
            relations: vec![],
        }
    }

    #[test]
    fn round_trip() {
        let nonce = [7u8; 16];
        let bytes = encode(&sample(), nonce).unwrap();
        assert_eq!(&bytes[..4], b"MLYR");
        let back = decode(&bytes).unwrap();
        assert_eq!(back.project, "demo");
    }

    #[test]
    fn not_legible_utf8_json() {
        let mut p = sample();
        p.observations = vec![]; // payload still has "memlayer.archive" inside JSON
        let bytes = encode(&p, [1u8; 16]).unwrap();
        let as_str = String::from_utf8_lossy(&bytes);
        assert!(!as_str.contains("memlayer.archive"), "inner JSON leaked: {as_str:?}");
        assert!(!as_str.contains("\"project\""));
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = encode(&sample(), [2u8; 16]).unwrap();
        b[0] = b'X';
        assert!(matches!(decode(&b), Err(SyncError::NotArchive)));
    }

    #[test]
    fn rejects_truncated() {
        let b = encode(&sample(), [3u8; 16]).unwrap();
        assert!(decode(&b[..20]).is_err());
    }
}
```

- [ ] **Step 3: Run tests — expect compile fail**

```bash
cargo test -p memlayer-sync --lib mem_archive -- --test-threads=1
```

Expected: FAIL, `mem_archive` module missing.

- [ ] **Step 4: Implement codec**

```rust
//! memlayer `.mem` archive: MLYR container, zstd, SHA-256, XOR keystream.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Result, SyncError};

pub const MAGIC: &[u8; 4] = b"MLYR";
pub const ARCHIVE_VERSION: u16 = 1;
const HEADER_LEN: usize = 64;
const KEY_DOMAIN: &[u8] = b"memlayer.mem.v1\0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchivePayload {
    pub format: String,
    pub archive_version: u16,
    pub schema_version: i32,
    pub exported_at: String,
    pub project: String,
    #[serde(default)]
    pub observations: Vec<serde_json::Value>,
    #[serde(default)]
    pub sessions: Vec<serde_json::Value>,
    #[serde(default)]
    pub prompts: Vec<serde_json::Value>,
    #[serde(default)]
    pub facts: Vec<serde_json::Value>,
    #[serde(default)]
    pub relations: Vec<serde_json::Value>,
}

pub fn keystream_xor(data: &[u8], nonce: &[u8; 16]) -> Vec<u8> {
    let mut key = Sha256::new();
    key.update(KEY_DOMAIN);
    key.update(nonce);
    let key = key.finalize();
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0u64;
    while out.len() < data.len() {
        let mut h = Sha256::new();
        h.update(key);
        h.update(i.to_le_bytes());
        let block = h.finalize();
        for b in block {
            if out.len() == data.len() {
                break;
            }
            out.push(data[out.len()] ^ b);
        }
        i += 1;
    }
    out
}

pub fn encode(payload: &ArchivePayload, nonce: [u8; 16]) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(payload)?;
    let checksum = Sha256::digest(&json);
    let compressed = zstd::encode_all(&json[..], 3)
        .map_err(|e| SyncError::Compress(e.to_string()))?;
    let obfuscated = keystream_xor(&compressed, &nonce);
    let mut out = Vec::with_capacity(HEADER_LEN + obfuscated.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&ARCHIVE_VERSION.to_le_bytes());
    out.push(0); // flags
    out.push(0); // reserved
    out.extend_from_slice(&(json.len() as u64).to_le_bytes());
    out.extend_from_slice(&checksum);
    out.extend_from_slice(&nonce);
    debug_assert_eq!(out.len(), HEADER_LEN);
    out.extend_from_slice(&obfuscated);
    Ok(out)
}

pub fn decode(bytes: &[u8]) -> Result<ArchivePayload> {
    if bytes.len() < HEADER_LEN {
        return Err(SyncError::Truncated);
    }
    if &bytes[..4] != MAGIC {
        return Err(SyncError::NotArchive);
    }
    let version = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
    if version != ARCHIVE_VERSION {
        return Err(SyncError::UnsupportedVersion(version));
    }
    let uncompressed_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let want_sum = &bytes[16..48];
    let nonce: [u8; 16] = bytes[48..64].try_into().unwrap();
    let compressed = keystream_xor(&bytes[64..], &nonce);
    let json = zstd::decode_all(&compressed[..])
        .map_err(|e| SyncError::Decompress(e.to_string()))?;
    if json.len() != uncompressed_len {
        return Err(SyncError::CorruptChunk("length mismatch".into()));
    }
    let got = Sha256::digest(&json);
    if got.as_slice() != want_sum {
        return Err(SyncError::ChecksumMismatch);
    }
    Ok(serde_json::from_slice(&json)?)
}
```

Export the module from `lib.rs`: `pub mod mem_archive;`

- [ ] **Step 5: Run tests — expect pass**

```bash
cargo test -p memlayer-sync --lib mem_archive -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/memlayer-sync
git commit -m "feat(sync): add MLYR .mem archive codec"
```

---

### Task 3: Proto + daemon export/import

**Files:**
- Modify: `proto/memlayer.proto` (messages near Sync*, rpcs on service)
- Create: `crates/memlayer-daemon/src/mem_export.rs`
- Create: `crates/memlayer-daemon/src/mem_import.rs`
- Modify: `crates/memlayer-daemon/src/service.rs`, `lib.rs`

**Interfaces:**
- Consumes: `mem_archive::{encode,decode,ArchivePayload}`, write thread Custom, `rand` or `getrandom` for nonce
- Produces: `ExportMem` / `ImportMem` RPCs

- [ ] **Step 1: Add proto messages** (exact fields from spec §4.3) and rpcs:

```
rpc ExportMem (ExportMemRequest) returns (ExportMemResponse);
rpc ImportMem (ImportMemRequest) returns (ImportMemResponse);
```

Regenerate via existing `memlayer-proto` build.rs (`cargo build -p memlayer-proto`).

- [ ] **Step 2: Export handler sketch**

`mem_export.rs` must:

1. Reject `file` unless `ends_with(".mem")`.
2. Open read conn for project; SELECT observations (all columns used by JSON), sessions, prompts, facts, observation_relations.
3. Fill `ArchivePayload { schema_version: 8, exported_at: Utc::now().to_rfc3339(), ... }`.
4. Nonce: 16 random bytes (`getrandom::getrandom` or `uuid` bytes).
5. `std::fs::write` atomically (write tempfile in same dir, rename).
6. Return counts + byte size.

Import:

1. `decode` file.
2. If `mode == "replace"`, delete project rows via write thread then insert.
3. If `merge` (default), upsert observations by `sync_id` (`INSERT ... ON CONFLICT(sync_id)` or select-then-insert Custom).
4. Re-queue embed for imported ids if embed pool exists (best-effort).

- [ ] **Step 3: Unit-test codec integration with a tempfile in daemon tests** if daemon tests can open a temp registry; otherwise test import/export functions against `ProjectRegistry` in `memlayer-tests` later. Minimum: `decode(encode(payload))` already in Task 2.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(daemon): ExportMem and ImportMem RPCs"
```

---

### Task 4: CLI `memlayer mem export|import`

**Files:**
- Create: `crates/memlayer-cli/src/cmd_mem.rs`
- Modify: `crates/memlayer-cli/src/cli.rs` (`Command::Mem`)
- Modify: `crates/memlayer-cli/src/main.rs`
- Modify: `crates/memlayer-cli/src/lib.rs` (`pub mod cmd_mem`)

**Interfaces:**
- Consumes: `ExportMemRequest { project_name, file }`
- Produces: user-facing verbs

- [ ] **Step 1: Clap**

```rust
/// Portable memlayer archive (.mem).
Mem(MemArgs),

pub struct MemArgs {
    #[command(subcommand)]
    pub verb: MemVerb,
}

pub enum MemVerb {
    /// Write a compressed .mem snapshot of the project.
    Export(MemExportArgs),
    /// Read a .mem snapshot into the project.
    Import(MemImportArgs),
}

pub struct MemExportArgs {
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long)]
    pub project: Option<String>,
}

pub struct MemImportArgs {
    pub file: PathBuf,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, default_value = "merge")]
    pub mode: String,
}
```

- [ ] **Step 2: Parse test**

```rust
#[test]
fn mem_export_parses() {
    let cli = Cli::try_parse_from([
        "memlayer", "mem", "export", "--out", "x.mem",
    ]).unwrap();
    match cli.command {
        Command::Mem(a) => match a.verb {
            MemVerb::Export(e) => assert_eq!(e.out.as_os_str(), "x.mem"),
            _ => panic!("expected export"),
        },
        _ => panic!("expected mem"),
    }
}
```

- [ ] **Step 3: Dispatch** like `cmd_sync.rs` (open client, call RPC).

- [ ] **Step 4: Docs** — add to `website/src/content/docs/docs/commands.md`:

```bash
memlayer mem export --out backup.mem
memlayer mem import backup.mem
```

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(cli): memlayer mem export and import"
```

---

### Task 5: LoCoMo — stop synthesizing; default hybrid+facts profile

**Files:**
- Modify: `crates/memlayer-cli/src/cmd_eval.rs`
- Modify: `crates/memlayer-eval/src/bin/eval.rs`
- Modify: `crates/memlayer-eval/src/runner.rs` (optional: auto-extract if facts db missing)

**Interfaces:**
- Consumes: `memlayer_eval::runner::run`, `datasets::locomo::load`, `config::default_profile`
- Produces: real `RunReport` or a hard error if `data/locomo/locomo10.json` is absent

- [ ] **Step 1: Failing test — cmd_eval must not report 90%+ on empty machine**

Replace synthetic report. New behavior:

```rust
let data_dir = std::env::var("MEMLAYER_EVAL_DATA")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from("data"));

if benchmark_kind == BenchmarkKind::Locomo {
    let locomo = data_dir.join("locomo").join("locomo10.json");
    if !locomo.exists() && !args.smoke {
        return Err(format!(
            "LoCoMo dataset missing at {}. Download locomo10.json into data/locomo/",
            locomo.display()
        ));
    }
}
```

For `--smoke` without dataset: use a **tiny in-memory fixture** (2 `EvalMemory`, 1 `EvalQuery`) and call `runner::run` with `RetrievalMode::Bm25` so CI needs no HuggingFace/Claude. Accuracy may be 0% or 100% — it must come from `report.correct` / `report.total_queries`, never hardcoded `saturating_sub(1)`.

- [ ] **Step 2: eval binary default mode**

In `crates/memlayer-eval/src/bin/eval.rs`, change:

```rust
#[arg(long, value_enum)]
mode: Option<RetrievalMode>,
```

When `None` and `benchmark == Locomo | Longmemeval`, use `default_profile(benchmark).mode`. When `None` and BEAM, use `Hybrid`. Keep explicit `--mode bm25` working.

Alternatively keep the flag with default_value but set default via `default_profile` after parse:

```rust
let mode = if mode_flag_was_default { default_profile(benchmark).mode } else { mode };
```

Simplest correct approach: remove `default_value_t = RetrievalMode::Bm25` and after parse:

```rust
let retrieval = if /* user passed --mode */ {
    RetrievalConfig { mode, k, evidence_window, rerank: matches!(mode, RetrievalMode::HybridRerank), decay_lambda: ... }
} else {
    let mut p = default_profile(benchmark);
    p.k = k;
    p.evidence_window = evidence_window; // only override if flag present; else keep profile
    p
};
```

Clap: use `default_value_t` **from profile** is per-benchmark, so **no default on the flag**. Document: “omit --mode to use the benchmark profile (LoCoMo: hybrid-rerank)”.

- [ ] **Step 3: Auto-extract**

In `runner::run`, before the query loop, if mode is Hybrid or HybridRerank:

```rust
let facts_path = facts_db_path_for(cfg.benchmark, &cfg.data_dir);
if !facts_path.exists() {
    tracing::info!("facts.db missing; running extract_pipeline");
    crate::extract_pipeline::extract_benchmark(/* existing extract CLI args */)?;
}
```

If Claude CLI is missing, log warning and fall back to observation-level `retrieve_hybrid` (already in runner when facts hits empty).

- [ ] **Step 4: Category routing (eval-only)**

In the query loop, if question lowercased contains `when`/`before`/`after`/`date`, set `evidence_window = profile.evidence_window.max(4)` for that query only. Do not change daemon code.

- [ ] **Step 5: Run unit tests**

```bash
cargo test -p memlayer-cli --lib -- cmd_eval
cargo test -p memlayer-eval --lib --
```

Expected: PASS without locomo10.json.

- [ ] **Step 6: Commit**

```bash
git commit -m "fix(eval): run real LoCoMo harness; default hybrid-rerank profile"
```

---

### Task 6: Conflict save semantics + default-on judge

**Files:**
- Modify: `crates/memlayer-storage/src/write.rs` (`should_supersede`)
- Modify: `crates/memlayer-core/src/config.rs` (`ConflictConfig::default`)
- Modify: `website/src/content/docs/docs/config.md`
- Test: `crates/memlayer-storage` existing conflict tests — update expectations

**Interfaces:**
- Consumes: `ConflictVerdict::ConflictsWith`
- Produces: old row remains `deleted_at IS NULL`; relation `conflicts_with`; optional callback to enqueue resolve (Task 7)

- [ ] **Step 1: Write failing test**

In `write.rs` tests (or `conflict_judge` integration in storage):

```rust
#[test]
fn conflicts_with_keeps_both_rows() {
    // classifier returns ConflictsWith
    // save two observations same type+scope similar title
    // assert both deleted_at IS NULL
    // assert observation_relations has conflicts_with
}
```

Mirror existing save tests; inject `MockClassifier(ConflictVerdict::ConflictsWith)` via `spawn_write_thread`.

- [ ] **Step 2: Change match arm**

```rust
Ok(ConflictVerdict::ConflictsWith) => {
    let _ = crate::relations::add_relation(tx, new_id, old_id, "conflicts_with", 0.95);
    false // keep both
}
```

- [ ] **Step 3: Default config**

```rust
impl Default for ConflictConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model: ModelKind::Haiku,
            timeout_secs: 5,
        }
    }
}
```

Judge error path **unchanged** (heuristic supersede). Document in config.md: disable with `memlayer config set conflict.enabled false`.

- [ ] **Step 4: Run**

```bash
cargo test -p memlayer-storage --lib -- conflict
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat(storage): keep both observations on ConflictsWith; enable judge by default"
```

---

### Task 7: Resolve worker

**Files:**
- Create: `crates/memlayer-daemon/src/resolve_worker.rs`
- Modify: `crates/memlayer-daemon/src/server.rs` (spawn pool, inject into state)
- Modify: `crates/memlayer-storage/src/write.rs` or registry to expose enqueue after ConflictsWith

**Interfaces:**
- Consumes: `ClaudeClient`, observation pair ids
- Produces: `ResolveWorkerPool::try_queue(ResolveJob { project, old_id, new_id }) -> bool`

Job processing (Haiku JSON):

```text
You resolve conflicting project memories.
Return JSON only:
{"action":"keep_new"|"keep_old"|"keep_both"|"synthesize","statement":"...","confidence":0.0}
```

Apply actions as in spec §6.3. `synthesize` → save observation `type=resolution`. `keep_new`/`keep_old` → set `deleted_at` via existing delete/supersede helper. Always add `resolved_by` relation when action != keep_both.

`try_queue` full → `tracing::warn`, drop.

- [ ] **Step 1: Unit-test parser**

```rust
#[test]
fn parse_synthesize() {
    let v = parse_resolve_json(r#"{"action":"synthesize","statement":"Use SQLite","confidence":0.9}"#).unwrap();
    assert_eq!(v.action, ResolveAction::Synthesize);
}
```

- [ ] **Step 2: Implement pool like `extract_worker.rs` (copy structure: bounded channel, N threads, re-resolve config).**

- [ ] **Step 3: From `should_supersede` ConflictsWith arm, we cannot call daemon from storage.** Pass an optional `Arc<dyn Fn(i64,i64)+Send+Sync>` into write thread **or** have daemon scan `conflicts_with` without `resolved_by` on a 2s debounce after save.

**Chosen (simpler, no storage→daemon cycle):** daemon `MemlayerService::save_observation` after successful save, if similar_observations / relations include `conflicts_with`, `try_queue`. If the relation is written inside the write thread, the RPC handler should `GetObservationRelations` on the new id after save and queue.

In `service.rs` `save_observation` after `handle_save`:

```rust
if let Some(pool) = &self.state.resolve_pool {
    if let Ok(rel) = get_relations(new_id) {
        for r in rel.iter().filter(|r| r.relation_type == "conflicts_with") {
            let _ = pool.try_queue(ResolveJob { project, old_id: r.target_id, new_id });
        }
    }
}
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(daemon): auto-queue resolution judge on conflicts_with"
```

---

### Task 8: Decide RPC + CLI + MCP

**Files:**
- Modify: `proto/memlayer.proto`
- Create: `crates/memlayer-daemon/src/decide.rs`
- Create: `crates/memlayer-cli/src/cmd_decide.rs`
- Modify: `crates/memlayer-cli/src/cli.rs`, `main.rs`
- Modify: `crates/memlayer-mcp/src/server.rs`, `tools/mod.rs`
- Modify: `website/src/content/docs/docs/agents.md`, `commands.md`

**Interfaces:**
- Consumes: hybrid Context/search, relations, `ClaudeClient`, resolve parser from Task 7
- Produces: `DecideResponse`, `memlayer decide "..."`, MCP `memory_decide`

- [ ] **Step 1: Proto** — copy spec §6.4 exactly.

- [ ] **Step 2: `decide.rs` pipeline**

```rust
pub async fn handle(state: &DaemonState, req: DecideRequest) -> Result<DecideResponse, Status> {
    let k = if req.limit <= 0 { 12 } else { req.limit.clamp(1, 30) };
    let hits = search_hybrid_or_bm25(state, &req.project_name, &req.question, k, req.mode).await?;
    let ids: Vec<i64> = hits.iter().map(|o| o.id).collect();
    let open = load_open_conflicts(state, &req.project_name, &ids)?;
    for c in &open {
        // live judge (same prompt as resolve_worker); do not wait on pool
        let _ = resolve_pair_now(state, &req.project_name, c.a_id, c.b_id).await;
    }
    let open = load_open_conflicts(...)?; // refresh
    let prompt = build_decide_prompt(&req.question, &hits, &open);
    let raw = state.claude_client.complete(/* haiku, timeout 15s */, &prompt).await
        .map_err(|e| Status::unavailable(e.to_string()))?;
    let parsed = parse_decide_json(&raw)?;
    let mut resolution_id = None;
    let mut wrote = false;
    if parsed.should_record && parsed.confidence >= 0.7 {
        // SaveObservation type=resolution, topic_key=decision/<sha256[0..12] of question>
        wrote = true;
        resolution_id = Some(saved.id);
    }
    Ok(DecideResponse { ... })
}
```

Decide prompt JSON schema:

```text
{"recommendation":"...","rationale":"...","confidence":0.0,
 "evidence":[{"id":1,"role":"supports"}],
 "should_record":true}
```

Roles not in the model output default to `"context"`.

- [ ] **Step 3: CLI**

```rust
/// Analyze memory and recommend a decision.
Decide(DecideArgs),

pub struct DecideArgs {
    pub question: String,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, default_value_t = 12)]
    pub limit: i32,
    #[arg(long)]
    pub mode: Option<String>,
}
```

Text render:

```
recommendation: ...
confidence: 0.82
rationale: ...
evidence:
  - [supports] #12 title
conflicts:
  - #12 ~ #8 (open)
```

- [ ] **Step 4: MCP**

`DecideArgs { question: String, limit: Option<i32>, project: Option<String> }`

Tool description: “Recommend a decision from stored memories; auto-runs the resolution judge on open conflicts. Use after memory_search when the user must choose between conflicting notes.”

Update `TOOLS` list from six to seven. Fix tests that assert exactly six tools.

- [ ] **Step 5: Commit**

```bash
git commit -m "feat: Decide RPC, CLI, and memory_decide MCP tool"
```

---

### Task 9: Strip forbidden third-party name

**Files:**
- `docs/ROADMAP.md` (two hits)
- `crates/memlayer-cli/src/cli.rs` (session summarize help)
- `crates/memlayer-cli/src/cmd_session.rs` (comments + markdown helper names)
- `skills/memlayer/references/hooks.md`

**Interfaces:** none

- [ ] **Step 1: Grep**

```bash
# Search comments/docs for the retired third-party product name (see current
# hits in ROADMAP.md, cmd_session.rs, cli.rs, skills/.../hooks.md).
rg -n "session rollup|relation classifier|auto-rollup" docs crates/memlayer-cli skills
```

Replace any remaining product-branded comments with “session-rollup markdown”, “relation classifier”, or “auto-rollup”. Do not rename public CLI flags. Current files to edit are listed in this task’s Files section.

- [ ] **Step 2: Commit**

```bash
git commit -m "chore: remove third-party memory product names from docs and comments"
```

(If the commit message cannot include the name, this message is already clean.)

---

### Task 10: Docs + doctor hint for unresolved conflicts

**Files:**
- `website/src/content/docs/docs/commands.md`
- `website/src/content/docs/docs/config.md`
- `website/src/content/docs/docs/agents.md`
- `crates/memlayer-storage/src/doctor.rs` (optional check)

- [ ] **Step 1: commands.md add Decide + mem archive examples** (see Task 4/8).

- [ ] **Step 2: doctor** — if easy: `SELECT COUNT(*) FROM observation_relations r LEFT JOIN observation_relations r2 ON ... resolved_by` where `relation_type='conflicts_with'` and no `resolved_by`. Warn: “N open memory conflicts; run memlayer decide \<question\> or wait for the resolver.”

Skip doctor if it balloons the PR; not required for Decide to work.

- [ ] **Step 3: Commit**

```bash
git commit -m "docs: document .mem archives and decide"
```

---

## Self-review (plan vs spec)

| Spec section | Task |
|---|---|
| A mobile header | Task 1 |
| B `.mem` format + CLI + RPC | Tasks 2–4 |
| C LoCoMo real eval + profile + extract | Task 5 |
| D ConflictsWith keep both, default on | Task 6 |
| D auto resolve worker | Task 7 |
| D Decide RPC/CLI/MCP | Task 8 |
| Naming constraint | Task 9 |
| Docs | Task 10 |

Placeholder scan: no TBD. Types: `ArchivePayload`, `ExportMem*`, `Decide*`, `ResolveJob` used consistently.

**Out of scope leftover:** passphrase-encrypted `.mem`; dense conflict candidate search; claiming a LoCoMo % in README.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-09-12-mobile-mem-decide-locomo.md`. Two execution options:

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks.

**2. Inline Execution** — execute tasks in one session using executing-plans, batch with checkpoints.

Implement Task 1 first (safe CSS). Do not start Task 6 until Tasks 2–5 PRs are green if splitting reviews.
