# TrendLab: Project Overview (v2)

*A foundation document for the detailed development roadmap*

---

## 0) One-sentence mission

TrendLab is a **research-grade trend-following backtesting lab** (not a trading engine) designed to answer, repeatedly and defensibly:

> “Which trend-following approach works, under what conditions, and how do I know I’m not fooling myself?” :contentReference[oaicite:1]{index=1}

It optimizes for:
- **fast parameter sweeps**
- **robustness / anti-overfit diagnostics**
- **exportable “Strategy Artifacts”** that enable **Pine Script parity verification** :contentReference[oaicite:2]{index=2}

---

## 1) Scope and non-goals (make this explicit early)

### In scope (Phase 1–2)
- Daily-bar research (EOD) with deterministic fill conventions
- Strategy-family screening across grids (thousands of configs)
- Portfolio accounting that is *correct-by-construction* (cash/position/equity invariants)
- Export artifacts that drive Pine generation + parity checking

### Explicit non-goals (for MVP)
- Live trading engine / broker integration
- Tick-level microstructure, queue priority, partial fills
- “Perfect realism” execution simulation
- Institutional-grade universe construction (we can *acknowledge* survivorship bias initially)

(These may be Phase 4+ topics; they are not allowed to leak into MVP design.)

---

## 2) Design principles (“pillars”), upgraded to enforceable contracts

### Pillar A — Correctness over cleverness
We will not accept speed if correctness is uncertain. This remains the #1 pillar. :contentReference[oaicite:3]{index=3}

**Hard invariants (must be tested):**
1. **No lookahead:** any value at time T may use only bars ≤ T.
2. **Determinism:** identical inputs must produce identical outputs.
3. **Accounting identity:** equity must match cash + marked-to-market positions at every bar.
4. **Explicit fill convention:** e.g., “signal computed on close, filled at next open” (default).

### Pillar B — Sweep-first research (families, not “best single config”)
We are hunting for **stable regions** in parameter space, not “the best point.” :contentReference[oaicite:4]{index=4}  
A strategy that works at 20 but fails at 19 and 21 is treated as suspicious.

### Pillar C — Pine Script is the verification layer
TrendLab exports **StrategyArtifact** JSON that fully describes:
- indicators + parameters
- entry/exit rules
- fill & cost assumptions
- **parity test vectors** (expected indicator values + signals on a small window) :contentReference[oaicite:5]{index=5}

TradingView can export Strategy Tester data (e.g., “List of Trades” tab) for verification workflows. :contentReference[oaicite:6]{index=6}

---

## 3) The end-to-end workflow (the “happy path”)

1) **Ingest & cache market data**
- Pull daily OHLCV (Yahoo Finance first)
- Cache raw provider responses (provenance preserved)
- Normalize into canonical Parquet partitions

2) **Build features + signals**
- Indicators → boolean signals (entry/exit)
- Strict time alignment (no accidental lookahead via shifts)

3) **Simulate fills + compute equity**
- Deterministic fill model (default: next-bar open)
- Explicit costs (fees/slippage parameters)

4) **Parameter sweep orchestration**
- Run grids across symbols/universes
- Store run manifests (config + git commit + data version)

5) **Robustness & ranking**
- OOS / walk-forward
- regime slices (high/low vol, bull/bear)
- parameter-surface stability
- cost sensitivity curves :contentReference[oaicite:7]{index=7}

6) **Export StrategyArtifact**
- Generate Pine-ready artifact + parity vectors
- Optional: generate a “Pine prompt packet” for LLM-assisted translation

7) **Pine parity verification**
- Generate Pine script from artifact
- Compare signals/trades against parity vectors and/or exported Strategy Tester data :contentReference[oaicite:8]{index=8}

---

## 4) Architecture (workspace-level)

### Crates (recommended)
- `trendlab-core`
  - domain types, indicators, strategies, sim engine, metrics
- `trendlab-cli`
  - orchestration + IO (data refresh, sweep runs, report export, artifact export)
- `trendlab-bdd`
  - cucumber runner + step definitions + fixtures (kept separate so prod crates stay clean)

**Why a dedicated BDD crate?**
Cucumber-rs expects a dedicated Rust test target with `harness = false` so it can print its own output, and feature files are tied to that test target. :contentReference[oaicite:9]{index=9}

---

## 5) Data contracts (tighten what “Bars are adjusted” means)

Your current contracts are good, but they need one more level of precision. :contentReference[oaicite:10]{index=10}

### Canonical Bar schema (must be versioned)
- `ts` (UTC; epoch ms or ns—choose one and lock it)
- `open, high, low, close` (f64)
- `volume` (u64 or f64)
- `symbol` (string)
- `timeframe` (e.g., “1d”)
- `source` (provider id)
- `asof` (fetch timestamp)
- `schema_version`

### Adjustments policy (must be explicit in code + docs)
Instead of “Bars are adjusted” as a blanket statement, we store:
- raw OHLCV (as provided)
- and (optionally) a clearly defined “adjusted_close” series *if provider supplies it*
- plus a flag describing which series is used for strategy evaluation

Reason: dividends/splits and provider differences can silently change results.

### Missing-bars policy
We treat missing bars as true gaps; lookbacks count trading days only (as you specified). :contentReference[oaicite:11]{index=11}  
But we also produce a data-quality report (duplicates, gaps, out-of-order timestamps) per refresh run.

### Performance posture (Polars)
Default to **LazyFrame + parquet scanning** so the optimizer can push down predicates/projections and reduce IO/memory. :contentReference[oaicite:12]{index=12}

---

## 6) Strategy families (what we must support)

This stays close to your plan, but with one key rule: **every strategy spec must define the fill convention and indicator alignment.**

### Phase 1 (MVP)
1. MA crossover (fast/slow)
2. Donchian breakout (N-day HH/LL breakout)

### Phase 2 (Core research)
3. Turtle System 1 (EOD approximation)
4. Turtle System 2 (EOD approximation)
5. Time-series momentum (TSMOM) with volatility scaling

### Phase 3+ (advanced variants)
- Multi-exit comparisons (same entry, different exits)
- Volatility sizing (ATR/N)
- Pyramiding

**Turtle reference note:** Original Turtle rules entered “at breakout when exceeded during the day” (intraday), not necessarily next open/close. TrendLab may approximate EOD; that approximation must be documented and tested. :contentReference[oaicite:13]{index=13}

---

## 7) Testing strategy (BDD + invariants + “parity tests”)

### BDD-first is not just a slogan
We will treat `.feature` scenarios as the product contract.

**Cucumber-rs integration requirements**
- Cucumber tests run via `cargo test`
- Requires a test target with `harness = false` in `Cargo.toml` :contentReference[oaicite:14]{index=14}

### What goes into BDD scenarios (minimum suite)
- No lookahead (indicator at T uses ≤T bars only)
- Fill convention correctness (next-open vs next-close)
- Accounting identity on every bar
- Determinism (same seed/config → same outputs)
- Stop precedence (if multiple exits trigger, deterministic ordering)
- Missing bar handling (gaps don’t get “filled in”)

### Fixture policy
- Keep tiny deterministic fixtures (20–200 bars) in-repo
- Keep big datasets out of git (Parquet cache is local)

### Pine parity tests
- StrategyArtifact includes parity vectors (expected signals/indicator values)
- Pine translation is “correct” only if parity vectors match (not vibes)

---

## 8) Outputs and artifacts (what “done” looks like)

For every sweep run, we output:
- `trades.parquet`
- `equity.parquet`
- `metrics.parquet`
- `summary.md` (top configs + notes)
- `run_manifest.json` (strategy id, parameters, costs, fill model, universe, dates, git commit hash, data version)
- `StrategyArtifact.json` for top configs (optional early, mandatory later)

---

## 9) Risks & mitigations (call them now)

1) **Data fragility (Yahoo)**
- Mitigation: raw caching, provenance logs, re-runnable normalization, optional secondary provider.

2) **Survivorship bias**
- Mitigation: document it early; later add constituent-aware universes.

3) **Silent lookahead bugs**
- Mitigation: BDD + parity vectors + invariant checks.

4) **Overfitting**
- Mitigation: stability tests are first-class outputs, not optional. :contentReference[oaicite:15]{index=15}

---

## 10) Success criteria (tightened + testable)

### Technical
- 100-config sweep < 60s on one symbol (baseline target) :contentReference[oaicite:16]{index=16}
- BDD suite covers invariants (no lookahead, determinism, accounting) :contentReference[oaicite:17]{index=17}
- StrategyArtifact → Pine script → parity vectors match

### Research
- Reproduce canonical Turtle-style behaviors within “EOD approximation” limits :contentReference[oaicite:18]{index=18}
- Parameter-surface stability identifies fragile configs
- Equal-footing comparisons (same costs/universe/dates)

### Practical
- Single command: tickers → sweep → top configs → artifacts → Pine prompts :contentReference[oaicite:19]{index=19}
- Docs good enough for a Rust-quant to add a new strategy safely

---

*This document is the contract for the roadmap: if an item isn’t testable or isn’t a stable contract, it doesn’t belong in MVP.*
