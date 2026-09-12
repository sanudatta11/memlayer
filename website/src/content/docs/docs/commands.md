---
title: Everyday commands
description: Common memlayer CLI commands for saving, searching, and managing memory.
---

```bash
memlayer obs save --type decision --title "..." --content "..." --session "$SID"
memlayer obs recent --limit 10
memlayer obs search "auth"                     # hybrid (BM25 + dense + RRF)
memlayer obs search "auth" --mode bm25         # lexical only
memlayer obs context --query "deploy" --limit 20
memlayer obs history <id>                      # supersession chain tree
memlayer obs relations <id>                    # graph relation edges
memlayer obs reindex [--force]                 # queue re-embedding and quantization
memlayer tui                                   # interactive observation browser
memlayer doctor [--repair]                     # database integrity audit & auto-repair
memlayer eval --smoke --save-scorecard card.json
memlayer decide "Should we keep SQLite or move to Postgres?"
memlayer config show
memlayer daemon status
memlayer logs --lines 50
memlayer mem export --out backup.mem
# prints a note about --seed-file / --seed-phrase
memlayer mem export --out secret.mem --seed-file ./phrase.txt
memlayer mem import backup.mem
memlayer mem import secret.mem --seed-file ./phrase.txt
```

TTY → text; pipes → JSON. Override with `--output {text,json,yaml}`.
