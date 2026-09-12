---
title: LoCoMo eval
description: Smoke and full LoCoMo runs for local analysis, plus how memlayer scores compare to published numbers.
---

Use the Makefile from the repo root. Smoke needs only a release binary. Full
needs `data/locomo/locomo10.json`, a BGE model directory, and an agent CLI for
answer + judge.

```bash
make eval-locomo-smoke          # fixture, BM25, lexical judge
make eval-locomo-fetch          # SNAP locomo10.json → ./data/locomo/
make eval-locomo                # full locomo10 e2e
make eval-locomo-e2e            # smoke then full
LIMIT=50 make eval-locomo       # cheaper slice
make eval-locomo-compare SCORECARD=eval/locomo-full.json
```

Scorecards are written to `eval/` (not committed). Equivalent CLI:

```bash
memlayer eval --smoke --benchmark locomo --save-scorecard eval/locomo-smoke.json
MEMLAYER_EVAL_DATA="$PWD/data" memlayer eval --benchmark locomo --save-scorecard eval/locomo-full.json
```

## What the numbers mean

| Run | Metric | Comparable to |
| --- | --- | --- |
| Smoke | Lexical contain on one fixture query | Nothing published |
| Full `accuracy_pct` | LLM-judge pass rate on locomo10 | Other locomo10 LLM-judge harnesses **if** judge model, k, and category filter match |
| Scorecard `f1_score` | Derived from accuracy | Not paper token F1 |
| Maharana et al. ACL 2024 | Token F1 on gold answers | Human 87.9, GPT-4-turbo 51.6 overall |

Public locomo10 LLM-judge tables usually land around **67–80%** overall when
adversarial items are dropped; some re-runs exceed **90%** with a stronger
judge or a different protocol. Retrieval **R@5** in the low-to-mid 90s is a
different question (was the evidence fetched?).

To beat or fairly test those bands: keep the default LoCoMo profile (hybrid +
rerank + evidence window), ingest speaker-prefixed turns, leave fact fusion
on, and disclose the answer/judge model. Do not quote smoke against a
leaderboard.
