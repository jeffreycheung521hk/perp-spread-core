# Local validation snapshot

Date: 2026-11-16. This records synthetic/local checks, not trading results.

- Toolchain: Rust 1.100.0; Python 3.12.14.
- `cargo fmt --all -- --check`.
- `cargo clippy --locked --offline --all-targets -- -D warnings`.
- `cargo test --locked --offline --all-targets`: 28 regression tests, no skipped tests.
- Independent Python `Decimal` comparison: 258 synthetic requests, including 191 valid quote comparisons and 67 expected errors.
- The reference checks VWAP display to 10^-12 USD, cash fees within four 10^-12 USD atoms, quote ratios within two micro-bps, and net edge within three micro-bps. It additionally checks that the Rust rounding does not overstate entry edge or understate estimated costs.

Regression coverage includes snapshot/delta/deletion, bid/ask traversal, top-20 depth, crossed books, invalid/overprecise numbers, duplicate/backward timestamps/IDs, stale/skewed quotes, equal-base sizing, four fees, changed exit fees, conservative signed EMA rounding, previous-observation EMA semantics, exact P&L reconciliation, invalid closing quantity/direction and overflow handling.

The demo has no market-data connection. Its manually constructed 10-SOL pair produces gross USD 3.60, four fees USD 1.902, and net USD 1.698 under hypothetical complete fills. These numbers are an accounting example and have no historical-performance meaning.

No live data, API signing, orders, partial-leg failures, funding, margin or speed benchmark was validated by these checks. The declared minimum Rust version is 1.74; this snapshot used the current stable compiler, not a separate MSRV matrix.
