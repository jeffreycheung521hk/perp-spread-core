# perp-spread-core

[![Rust CI](https://github.com/jeffreycheung521hk/perp-spread-core/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/jeffreycheung521hk/perp-spread-core/actions/workflows/ci.yml)

A Rust core for cross-exchange perpetual futures spread analysis, order-book processing, and trading-cost modeling.

The first version models a SOL-sized pair between Hyperliquid and dYdX. It reconstructs books from supplied snapshots and deltas, walks executable depth, calculates four taker fees, and evaluates a candidate spread against a historical exit-cost estimate. It is a research library and a deterministic command-line calculator; it has no exchange connections, wallet keys, or real-order submission.

[繁體中文說明](docs/README.zh-TW.md) · [TSV interface](docs/PROTOCOL.md) · [Validation snapshot](docs/VALIDATION.md)

## Quick start

Install Rust, then run from the repository root:

```bash
cargo test --locked --offline --all-targets
cargo run --locked --offline -- --demo
```

There are **no third-party Rust dependencies**. The demo uses deliberately constructed prices. It is not historical data, a live trade, or evidence of a profitable strategy.

To compare the release calculator with an independent Python `Decimal` reference:

```bash
cargo build --locked --offline --release
python3 scripts/check_python_parity.py --binary target/release/spread-core
```

Python 3.9+ is sufficient; the verification script also uses only the standard library.

## What the core does

| Module | Responsibility |
|---|---|
| `src/fixed.rs` | Decimal parsing, typed prices/sizes/cash/bps, checked arithmetic, taker fees |
| `src/book.rs` | Ordered bid/ask maps, snapshot/delta handling, top-20 walking, VWAP display, slippage |
| `src/strategy.rs` | Equal-base sizing, four-fill cost estimate, freshness/liquidity filters, EMA, hypothetical P&L |
| `src/main.rs` | Synthetic demo and persistent TSV input/output for verification |
| `tests/core.rs` | Financial, temporal, invalid-data and overflow regression tests |
| `scripts/check_python_parity.py` | Independent high-precision Decimal comparison using synthetic fixtures |

Buying walks asks from low to high; selling walks bids from high to low. If the first 20 levels cannot fill the whole requested size, the calculation returns an error. A higher ask alone is not treated as a profitable short-sale opportunity.

Both legs use the same base-asset quantity. If quantity is automatic, the budget divided by the average of the four best quotes determines a size, rounded down to the configured step. If quantity is supplied explicitly, it overrides budget-based sizing, which is useful when estimating closure of an existing pair. Prices and sizes must be expressed in consistent USD/base units by the caller; the library does not fetch or validate exchange contract specifications.

## Cost and P&L definitions

All bps use a **single-leg reference notional**. The reference price is the average of the two venue midprices; the arithmetic retains that reference's exact denominator.

```text
estimated net edge
  = executable entry credit
  - previous EMA estimate of exit price debit
  - estimated fees for all four fills
  - configurable buffer

hypothetical closing P&L
  = long exit proceeds - long entry cost
  + short entry proceeds - short exit cost
  - entry fees - exit fees
```

Fees use each fill's own notional. Closing estimates use the supplied exit quote's fee rates, so a fee-tier change can be represented. VWAP is formatted for display; accounting keeps exact walked notional and does not calculate cash by multiplying a rounded average price. Walked-price slippage is not charged twice.

The sample 4.5/5 bps fees and policy thresholds are **illustrative inputs**, not verified account rates or optimized trading parameters. The buffer is an entry filter, not an expense that was actually paid.

## Numerical rules

- Prices: six decimal places in USD/base; quantities: six decimal places in base units.
- Cash: twelve decimal places in USD. Bps: six decimal places.
- Exact decimal strings enter checked `i128` arithmetic; there is no financial `f64` calculation.
- Excess precision, scientific notation, non-finite values, invalid signs, zero trade size and arithmetic overflow return explicit errors.
- Fees round upward by less than one cash atom per fill. Positive edge is rounded down; estimated costs and slippage are rounded up at micro-bp precision.
- EMA rounds its estimated cost upward at micro-bp precision. It is an approximation and returns the estimate from **before** incorporating the current observation. The caller controls sampling cadence, warmup usage, and resets after gaps.

## Data validity and scope

`OrderBook::apply` supports snapshots, incremental price-level updates, and zero-size deletion. Rejected updates invalidate the previous book. A new snapshot is then required. Provided duplicate/backward message IDs, backward source timestamps, and backward arrival times are rejected.

Both venues' `received_ms` values must use the same local monotonic clock. `Quote::assess` filters future/stale arrivals, pair skew, venue spread, walked slippage and insufficient estimated net edge. The caller is responsible for source-data age, clock synchronization, venue-specific message-ID semantics and sequence-gap detection. The library does not claim that receipt time equals exchange matching time.

This version does **not** implement:

- WebSocket/API adapters or integration into the earlier Python collector.
- Orders, independent/partial leg fills, cancellations, rejected-order recovery or emergency hedging.
- End-to-end latency simulation, a continuous trading engine or an account ledger.
- Funding, margin, leverage, liquidations, capital costs, cloud costs or transfers.
- Live fee tiers, lot/tick/minimum-notional rules, or exchange-specific contract checks.
- Measured speedup, out-of-sample returns, strategy capacity or live profitability.

`paper_open().mark_close()` is an arithmetic estimate assuming complete fills of both legs. It does not mutate an account or execute a trade. A production caller must validate fresh data for both entry and exit and preserve actual position state independently of this calculator.

## Development checks

```bash
cargo fmt --all -- --check
cargo clippy --locked --offline --all-targets -- -D warnings
cargo test --locked --offline --all-targets
cargo build --locked --offline --release
python3 scripts/check_python_parity.py --binary target/release/spread-core
```

GitHub Actions runs these checks on pushes to `main` and on pull requests. The Python reference verifies quote arithmetic and filters; it is not a full replay of the previous Python paper-trading engine. Rust regression tests separately cover EMA ordering and hypothetical trade reconciliation.

## License

Licensed under the [MIT License](LICENSE). Copyright (c) 2026 Jeffrey Cheung.
