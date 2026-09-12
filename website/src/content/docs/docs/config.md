---
title: Config
description: memlayer features and configuration overlays.
---

`memlayer install` writes `~/.memlayer/config.toml` with hybrid search, the
conflict judge, and extract enabled. Code defaults also enable
`conflict.enabled` and `search.mode = "hybrid"`. Extract stays opt-in in
code defaults (no Claude on unit tests) but install turns it on.

Disable the judge if you want heuristic supersession only:

```bash
memlayer config set conflict.enabled false
memlayer config set extract.enabled false    # skip fact triples on save
memlayer config set search.mode bm25         # lexical-only retrieval
memlayer config set embed.quantize true      # int8 vectors (~75% smaller)
memlayer obs reindex                         # backfill embeddings
```

## Config files

- Global: `~/.memlayer/config.toml`
- Per-project overlay: `~/.memlayer/projects/<name>.config.toml`

Merge order (highest wins): env vars → project overlay → global → code defaults.

## Common env overrides

`MEMLAYER_EXTRACT_ENABLED`, `MEMLAYER_EXTRACT_MODEL`, `MEMLAYER_EXTRACT_TIMEOUT_SECS`,
`MEMLAYER_EXTRACT_WORKERS`, `MEMLAYER_RERANK_MODEL`, `MEMLAYER_RERANK_TIMEOUT_SECS`,
`MEMLAYER_EMBED_WORKERS`, `MEMLAYER_EMBED_QUANTIZE`,
`MEMLAYER_CONFLICT_ENABLED`, `MEMLAYER_CONFLICT_MODEL`, `MEMLAYER_CONFLICT_TIMEOUT_SECS`,
`MEMLAYER_CLAUDE_MODEL` (raw `claude --model` id; tried first when Haiku/Sonnet are not installed).

See the repository [`CLAUDE.md`](https://github.com/sanudatta11/memlayer/blob/main/CLAUDE.md)
for the full knob list and architecture notes.
