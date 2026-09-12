# memlayer-eval

Benchmark harness for LoCoMo, LongMemEval, and BEAM. The daemon does not
depend on this crate.

## Local LoCoMo (preferred)

From the repo root:

```bash
# Wiring check: BM25 + lexical judge, no dataset, no LLM
make eval-locomo-smoke

# Fetch SNAP locomo10.json into ./data/locomo/
make eval-locomo-fetch

# Full e2e: ingest + hybrid-rerank + LLM answer/judge (needs BGE + agent CLI)
make eval-locomo

# Smoke then full
make eval-locomo-e2e

# Cheaper full slice
LIMIT=50 make eval-locomo

# Re-print a saved scorecard against published bands
make eval-locomo-compare SCORECARD=eval/locomo-full.json
```

Scorecards land in `eval/` (gitignored). Published reference numbers live in
`baselines/locomo.json`.

`make extract-locomo` / `make run-locomo` still drive the older `eval` binary
inside this crate (conv-26 extract, `--limit 200`). Use the `eval-locomo*`
targets for CLI e2e and local analysis.

## Metrics

memlayer `--save-scorecard` writes `accuracy_pct` (judge or lexical pass rate)
and an F1 derived from that accuracy. That is **not** Maharana et al. token F1
(human 87.9, GPT-4-turbo 51.6). Public locomo10 LLM-judge leaderboards are a
closer family of metric, but only if judge model, k, and adversarial inclusion
match. See `baselines/locomo.json`.
