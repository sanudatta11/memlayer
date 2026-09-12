# Mobile brand, `.mem` archive, LoCoMo wiring, Decide Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the clipped mobile `memlayer` header; add a native compressed `.mem` archive; make LoCoMo eval actually run hybrid+facts retrieval; add Decide plus auto resolution on conflicting memories — without naming any third-party memory product anywhere in the tree.

**Architecture:** Four sequential PRs. CSS-only header fix. Archive codec in `memlayer-sync` (MLYR + MessagePack + zstd-19; default XOR wrap; optional Argon2id + XChaCha20-Poly1305 from a seed phrase) with daemon snapshot/import. Eval CLI calls `runner::run` and defaults to `default_profile(Locomo)`. Save path keeps `ConflictsWith` rows, queues `resolve_worker`; `Decide` RPC retrieves, judges open conflicts, returns a structured recommendation and may write `type=resolution`.

**Tech Stack:** Starlight CSS, Rust, proto3/tonic, rusqlite, zstd 19, MessagePack (`rmp-serde`), Argon2id, XChaCha20-Poly1305, SHA-256, existing `ClaudeClient` shell-out, rmcp MCP tools.

## Global Constraints

- Never write the substring matching a third-party memory product name in code, comments, prompts, docs, CLI help, or commits. Use **Decide**, **resolution judge**, **memlayer archive**.
- Default `.mem` is obfuscation (XOR after zstd). Optional `--seed-phrase` / `--seed-file` uses Argon2id + XChaCha20-Poly1305. Never call the default path “encrypted”. Never log the seed. After every successful `mem export` and `mem import`, print the seed-encryption note to stderr. Importing a seed-encrypted file without `--seed-file`/`--seed-phrase` is a hard error with a dedicated message (no TTY prompt, no AEAD/zstd internals).
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
- Consumes: `zstd` 19, `sha2`, `rmp-serde`, `argon2`, `chacha20poly1305`, `zeroize`, `unicode-normalization`
- Produces:
  - `pub const MAGIC: &[u8; 4] = b"MLYR";`
  - `pub const ARCHIVE_VERSION: u16 = 1;`
  - `pub const HEADER_LEN: usize = 88;`
  - `pub const FLAG_ENCRYPTED: u8 = 0x01;`
  - `pub struct ArchivePayload { ... }`
  - `pub struct EncodeParams { pub salt: [u8; 16], pub aead_nonce: [u8; 24], pub seed: Option<String> }`
  - `pub fn encode(payload: &ArchivePayload, params: &EncodeParams) -> Result<Vec<u8>>`
  - `pub fn decode(bytes: &[u8], seed: Option<&str>) -> Result<ArchivePayload>`
  - `pub fn is_encrypted(bytes: &[u8]) -> Result<bool>`
  - `pub fn normalize_seed(seed: &str) -> Result<String>`

- [ ] **Step 0: Add crate deps** in `crates/memlayer-sync/Cargo.toml`:

```toml
argon2 = "0.5"
chacha20poly1305 = "0.10"
rmp-serde = "1.3"
zeroize = { version = "1", features = ["derive"] }
unicode-normalization = "0.1"
```

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

#[error("seed phrase must be at least 12 characters after trim")]
SeedTooShort,

#[error("this archive is seed-encrypted and cannot be imported without the seed phrase")]
SeedRequired,

#[error("invalid seed phrase or corrupt archive")]
InvalidSeed,
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

    fn params(seed: Option<&str>) -> EncodeParams {
        EncodeParams {
            salt: [7u8; 16],
            aead_nonce: [9u8; 24],
            seed: seed.map(|s| s.to_string()),
        }
    }

    #[test]
    fn round_trip_default() {
        let bytes = encode(&sample(), &params(None)).unwrap();
        assert_eq!(&bytes[..4], b"MLYR");
        assert_eq!(bytes[6] & FLAG_ENCRYPTED, 0);
        let back = decode(&bytes, None).unwrap();
        assert_eq!(back.project, "demo");
    }

    #[test]
    fn round_trip_seeded() {
        let seed = "correct horse battery staple extra";
        let bytes = encode(&sample(), &params(Some(seed))).unwrap();
        assert_ne!(bytes[6] & FLAG_ENCRYPTED, 0);
        let back = decode(&bytes, Some(seed)).unwrap();
        assert_eq!(back.project, "demo");
        assert!(matches!(decode(&bytes, None), Err(SyncError::SeedRequired)));
        assert!(format!("{}", decode(&bytes, None).unwrap_err())
            .contains("cannot be imported without the seed phrase"));
        assert!(matches!(
            decode(&bytes, Some("wrong seed phrase!!")),
            Err(SyncError::InvalidSeed)
        ));
    }

    #[test]
    fn encrypted_without_seed_is_seed_required_not_corrupt() {
        let bytes = encode(&sample(), &params(Some("correct horse battery staple extra"))).unwrap();
        let err = decode(&bytes, None).unwrap_err();
        assert!(matches!(err, SyncError::SeedRequired));
        let s = err.to_string();
        assert!(!s.to_lowercase().contains("aead"));
        assert!(!s.to_lowercase().contains("checksum"));
    }

    #[test]
    fn not_legible() {
        let bytes = encode(&sample(), &params(None)).unwrap();
        let as_str = String::from_utf8_lossy(&bytes);
        assert!(!as_str.contains("memlayer.archive"));
        assert!(!as_str.contains("demo"));
    }

    #[test]
    fn smaller_than_json_zstd3() {
        let mut p = sample();
        p.observations = (0..50)
            .map(|i| serde_json::json!({"id": i, "title": format!("obs-{i}-repeated-text")}))
            .collect();
        let mem = encode(&p, &params(None)).unwrap();
        let json = serde_json::to_vec_pretty(&p).unwrap();
        let z3 = zstd::encode_all(&json[..], 3).unwrap();
        assert!(mem.len() < z3.len(), "mem {} vs json.zst-3 {}", mem.len(), z3.len());
    }

    #[test]
    fn rejects_bad_magic() {
        let mut b = encode(&sample(), &params(None)).unwrap();
        b[0] = b'X';
        assert!(matches!(decode(&b, None), Err(SyncError::NotArchive)));
    }

    #[test]
    fn rejects_truncated() {
        let b = encode(&sample(), &params(None)).unwrap();
        assert!(decode(&b[..20], None).is_err());
    }

    #[test]
    fn seed_too_short() {
        assert!(encode(&sample(), &params(Some("short"))).is_err());
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
//! memlayer `.mem` archive: MLYR + MessagePack + zstd-19.
//! Default: XOR obfuscation. Optional: Argon2id + XChaCha20-Poly1305.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    Key, XChaCha20Poly1305, XNonce,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;
use zeroize::Zeroizing;

use crate::error::{Result, SyncError};

pub const MAGIC: &[u8; 4] = b"MLYR";
pub const ARCHIVE_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 88;
pub const FLAG_ENCRYPTED: u8 = 0x01;
const ZSTD_LEVEL: i32 = 19;
const KEY_DOMAIN: &[u8] = b"memlayer.mem.v1\0";
const ARGON2_M_KIB: u32 = 64 * 1024;
const ARGON2_T: u32 = 3;
const ARGON2_P: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ArchivePayload {
    pub format: String,
    pub archive_version: u16,
    pub schema_version: i32,
    pub exported_at: String,
    pub project: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub observations: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sessions: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prompts: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<serde_json::Value>,
}

pub struct EncodeParams {
    pub salt: [u8; 16],
    pub aead_nonce: [u8; 24],
    pub seed: Option<String>,
}

pub fn normalize_seed(seed: &str) -> Result<String> {
    let n: String = seed.nfc().collect();
    let n = n.trim().trim_end_matches(['\n', '\r']).to_string();
    if n.chars().count() < 12 {
        return Err(SyncError::SeedTooShort);
    }
    Ok(n)
}

fn derive_key(seed: &str, salt: &[u8; 16]) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(ARGON2_M_KIB, ARGON2_T, ARGON2_P, Some(32))
        .map_err(|e| SyncError::Compress(e.to_string()))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(seed.as_bytes(), salt, &mut out[..])
        .map_err(|_| SyncError::InvalidSeed)?;
    Ok(out)
}

pub fn keystream_xor(data: &[u8], salt: &[u8; 16]) -> Vec<u8> {
    let mut key = Sha256::new();
    key.update(KEY_DOMAIN);
    key.update(salt);
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

fn pack_header(flags: u8, inner_len: u64, checksum: &[u8], salt: &[u8; 16], aead_nonce: &[u8; 24]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&ARCHIVE_VERSION.to_le_bytes());
    out.push(flags);
    out.push(0);
    out.extend_from_slice(&inner_len.to_le_bytes());
    out.extend_from_slice(checksum);
    out.extend_from_slice(salt);
    out.extend_from_slice(aead_nonce);
    debug_assert_eq!(out.len(), HEADER_LEN);
    out
}

pub fn is_encrypted(bytes: &[u8]) -> Result<bool> {
    if bytes.len() < HEADER_LEN {
        return Err(SyncError::Truncated);
    }
    if &bytes[..4] != MAGIC {
        return Err(SyncError::NotArchive);
    }
    Ok(bytes[6] & FLAG_ENCRYPTED != 0)
}

pub fn encode(payload: &ArchivePayload, params: &EncodeParams) -> Result<Vec<u8>> {
    let inner = rmp_serde::to_vec_named(payload).map_err(|e| SyncError::Compress(e.to_string()))?;
    let checksum = Sha256::digest(&inner);
    let compressed = zstd::encode_all(&inner[..], ZSTD_LEVEL)
        .map_err(|e| SyncError::Compress(e.to_string()))?;
    let mut flags = 0u8;
    let body = if let Some(raw) = &params.seed {
        flags |= FLAG_ENCRYPTED;
        let seed = normalize_seed(raw)?;
        let key = derive_key(&seed, &params.salt)?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key[..]));
        cipher
            .encrypt(XNonce::from_slice(&params.aead_nonce), compressed.as_ref())
            .map_err(|_| SyncError::InvalidSeed)?
    } else {
        keystream_xor(&compressed, &params.salt)
    };
    let mut out = pack_header(flags, inner.len() as u64, &checksum, &params.salt, &params.aead_nonce);
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode(bytes: &[u8], seed: Option<&str>) -> Result<ArchivePayload> {
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
    let encrypted = bytes[6] & FLAG_ENCRYPTED != 0;
    let uncompressed_len = u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize;
    let want_sum = &bytes[16..48];
    let salt: [u8; 16] = bytes[48..64].try_into().unwrap();
    let aead_nonce: [u8; 24] = bytes[64..88].try_into().unwrap();
    let body = &bytes[88..];
    let compressed = if encrypted {
        let Some(raw) = seed else {
            return Err(SyncError::SeedRequired);
        };
        let seed = normalize_seed(raw)?;
        let key = derive_key(&seed, &salt)?;
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&key[..]));
        cipher
            .decrypt(XNonce::from_slice(&aead_nonce), body)
            .map_err(|_| SyncError::InvalidSeed)?
    } else {
        keystream_xor(body, &salt)
    };
    let inner = zstd::decode_all(&compressed[..])
        .map_err(|e| SyncError::Decompress(e.to_string()))?;
    if inner.len() != uncompressed_len {
        return Err(SyncError::CorruptChunk("length mismatch".into()));
    }
    let got = Sha256::digest(&inner);
    if got.as_slice() != want_sum {
        return Err(SyncError::ChecksumMismatch);
    }
    rmp_serde::from_slice(&inner).map_err(|e| SyncError::Compress(e.to_string()))
}
```

If `Argon2::hash_password_into` is not on the 0.5 API, use the crate’s low-level `hash_password_into` associated with the constructed `Argon2` instance (same parameters). Do not add a second KDF.

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
- Consumes: `mem_archive::{encode,decode,is_encrypted,ArchivePayload,EncodeParams}`, write thread Custom, `getrandom` for salt + aead nonce
- Produces: `ExportMem` / `ImportMem` RPCs (seed_phrase optional on both requests)

- [ ] **Step 1: Add proto messages** (exact fields from spec §4.3) and rpcs:

```
rpc ExportMem (ExportMemRequest) returns (ExportMemResponse);
rpc ImportMem (ImportMemRequest) returns (ImportMemResponse);
```

Regenerate via existing `memlayer-proto` build.rs (`cargo build -p memlayer-proto`).

- [ ] **Step 2: Export handler sketch**

`mem_export.rs` must:

1. Reject `file` unless `ends_with(".mem")`.
2. Open read conn for project; SELECT observations, sessions, prompts, facts, observation_relations (no embedding blobs).
3. Fill `ArchivePayload { schema_version: 8, exported_at: Utc::now().to_rfc3339(), ... }`.
4. Fill `EncodeParams`: 16-byte salt + 24-byte XChaCha nonce from `getrandom`. Pass `req.seed_phrase` if non-empty.
5. `std::fs::write` atomically (write tempfile in same dir, rename). Never `tracing` the seed.
6. Return counts + byte size.

Import:

1. Peek `is_encrypted`. If true and `seed_phrase` is empty, **do not decode**. Return `tonic::Status::invalid_argument` with:
   `this archive is seed-encrypted and cannot be imported without the seed phrase. pass --seed-file or --seed-phrase (the same phrase used at export).`
   Map `SyncError::SeedRequired` to this same status if decode is reached anyway.
2. If false and seed was sent, still decode without the seed (ignore extra seed). Include `warning_seed_ignored` in logs only as a boolean — never the phrase.
3. `decode(&bytes, seed_phrase.as_deref())` when encrypted.
4. If `mode == "replace"`, delete project rows via write thread then insert.
5. If `merge` (default), upsert observations by `sync_id`.
6. Re-queue embed for imported ids if embed pool exists (best-effort).
7. Response includes `bool seed_encrypted` so the CLI can print the matching note even if it did not peek.

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
- Consumes: `ExportMemRequest { project_name, file, seed_phrase }`
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
    /// Default is obfuscated only. Pass --seed-file or --seed-phrase to encrypt.
    Export(MemExportArgs),
    /// Read a .mem snapshot into the project.
    /// Seed-encrypted files require --seed-file or --seed-phrase.
    Import(MemImportArgs),
}

pub struct MemExportArgs {
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long)]
    pub project: Option<String>,
    /// Encrypt the archive with this seed phrase (same phrase decrypts on import).
    #[arg(long, conflicts_with = "seed_file")]
    pub seed_phrase: Option<String>,
    /// Read the seed phrase from a file (preferred; avoids `ps` leakage).
    #[arg(long)]
    pub seed_file: Option<PathBuf>,
}

pub struct MemImportArgs {
    pub file: PathBuf,
    #[arg(long)]
    pub project: Option<String>,
    #[arg(long, default_value = "merge")]
    pub mode: String,
    #[arg(long, conflicts_with = "seed_file")]
    pub seed_phrase: Option<String>,
    #[arg(long)]
    pub seed_file: Option<PathBuf>,
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

#[test]
fn mem_export_seed_file_parses() {
    let cli = Cli::try_parse_from([
        "memlayer", "mem", "export", "--out", "x.mem", "--seed-file", "phrase.txt",
    ]).unwrap();
    match cli.command {
        Command::Mem(a) => match a.verb {
            MemVerb::Export(e) => {
                assert_eq!(e.seed_file.as_deref(), Some(std::path::Path::new("phrase.txt")));
                assert!(e.seed_phrase.is_none());
            }
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
# prints a note about --seed-file / --seed-phrase
memlayer mem export --out secret.mem --seed-file ./phrase.txt
memlayer mem import backup.mem
memlayer mem import secret.mem --seed-file ./phrase.txt
```

CLI must read `--seed-file` (trim newline), reject both flags together, pass the string only in the gRPC request, and never print it.

- [ ] **Step 4b: Post-export note (required)**

Add in `cmd_mem.rs`:

```rust
const HINT_UNENCRYPTED: &str = "\
note: archive is not seed-encrypted (obfuscated only).\n\
      encrypt with a seed phrase (same phrase required on import):\n\
        memlayer mem export --out FILE.mem --seed-file ./phrase.txt\n\
        memlayer mem export --out FILE.mem --seed-phrase 'your phrase here'\n\
      keep the phrase; it cannot be recovered.";

const HINT_ENCRYPTED: &str = "\
note: archive is seed-encrypted. import needs the same --seed-file or --seed-phrase.\n\
      if you lose the phrase, this file cannot be opened.";
```

After a successful `export` RPC, `eprintln!` `HINT_UNENCRYPTED` when neither seed flag was set, else `HINT_ENCRYPTED`. Always print (including non-TTY). JSON render adds `seed_encrypted` and `hint` (first line only).

On `import`, peek with `is_encrypted(&std::fs::read(&file)?)?` before the RPC:

```rust
const ERR_SEED_REQUIRED: &str = "\
error: this archive is seed-encrypted and cannot be imported without the seed phrase.
       pass the same phrase used at export:
         memlayer mem import FILE.mem --seed-file ./phrase.txt
         memlayer mem import FILE.mem --seed-phrase 'your phrase here'";
```

If encrypted and both seed flags are absent: print `ERR_SEED_REQUIRED` (substitute `FILE.mem` with the path), return the usage exit code, **do not** call ImportMem. If the daemon still returns `SeedRequired`, print the same text (never the raw tonic message if it contains zstd/AEAD details).

After successful import: `HINT_IMPORT_ENCRYPTED` or `HINT_IMPORT_UNENCRYPTED` from the spec; if `seed_ignored`, also warn that the phrase was unused.

Unit test:

```rust
#[test]
fn unencrypted_hint_mentions_seed_flags() {
    assert!(HINT_UNENCRYPTED.contains("--seed-file"));
    assert!(HINT_UNENCRYPTED.contains("--seed-phrase"));
    assert!(HINT_UNENCRYPTED.contains("not seed-encrypted"));
}

#[test]
fn import_without_seed_error_is_actionable() {
    assert!(ERR_SEED_REQUIRED.contains("seed-encrypted"));
    assert!(ERR_SEED_REQUIRED.contains("cannot be imported without the seed phrase"));
    assert!(ERR_SEED_REQUIRED.contains("--seed-file"));
    assert!(ERR_SEED_REQUIRED.contains("--seed-phrase"));
    assert!(!ERR_SEED_REQUIRED.to_lowercase().contains("aead"));
    assert!(!ERR_SEED_REQUIRED.to_lowercase().contains("zstd"));
}
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

**Out of scope leftover:** BIP39 wordlist validation; dense conflict candidate search; claiming a LoCoMo % in README.

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-09-12-mobile-mem-decide-locomo.md`. Two execution options:

**1. Subagent-Driven (recommended)** — dispatch a fresh subagent per task, review between tasks.

**2. Inline Execution** — execute tasks in one session using executing-plans, batch with checkpoints.

Implement Task 1 first (safe CSS). Do not start Task 6 until Tasks 2–5 PRs are green if splitting reviews.
