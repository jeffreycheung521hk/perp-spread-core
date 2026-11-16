# Local validation snapshot

Date: 2026-11-16. This records synthetic/local checks, not trading results.

- Toolchains: Rust 1.100.0 and Rust 1.74.0; Python 3.12.14.
- `cargo fmt --all -- --check`.
- `cargo clippy --locked --offline --all-targets -- -D warnings`.
- `cargo test --locked --offline --all-targets`: 50 tests (30 core, 12 probe-derived/API, 6 real-binary CLI, 2 N1 delta-gap), no skipped tests.
- `cargo test --locked --offline --doc`: three compile-fail tests prevent rewriting Quote, ExitQuote and Fill financial fields.
- Independent Python `Decimal` comparison: 258 synthetic requests, including 191 valid quote comparisons and 67 expected errors.
- The reference checks VWAP display to 10^-12 USD, cash fees within four 10^-12 USD atoms, quote ratios within two micro-bps, and net edge within three micro-bps. It additionally checks that the Rust rounding does not overstate entry edge or understate estimated costs.

Regression coverage includes snapshot/delta/deletion, retained crossed state and natural recovery, scoped sequence resets, persistent source watermarks, ignored duplicate IDs without freshness extension, time-rejected deltas requiring resnapshot, rejected snapshots preserving prior depth, duplicate snapshot prices, exit-only depth, budget-free fixed sizing, immutable quote boundaries, byte framing after blank/non-UTF-8 input, bid/ask traversal, top-20 depth, invalid/overprecise numbers, stale/skewed quotes, four fees, changed exit fees, signed EMA rounding, previous-observation EMA semantics, exact P&L reconciliation and overflow. Both N1 tests failed on the preceding implementation before the fix; they now verify that a rejected deletion cannot leave a phantom order quotable, including from Crossed state.

The supplied 10 original probes were first run against the unmodified core and all reproduced their described current behavior. Only the selected high/medium-priority behaviors and the related duplicate-snapshot case were converted into fixed-behavior regressions. The original upload was not edited. P6/P9 and the remaining low-priority issues are tracked in REVIEW_FOLLOWUPS.md.

The 258-request Python reference still samples four of the six rejection filters. It does not cross-validate walk-slippage/venue-spread rejection or all error reasons, and its assertions are not effective under `python -O`. The two unrepresented filters have Rust regression tests. This pass does not claim these parity limitations are fixed.

The demo has no market-data connection. Its manually constructed 10-SOL pair produces gross USD 3.60, four fees USD 1.902, and net USD 1.698 under hypothetical complete fills. These numbers are an accounting example and have no historical-performance meaning.

The declared MSRV was manually verified with Rust 1.74.0: all 50 runtime tests and all three compile-fail doctests passed. A dedicated CI MSRV job and pinned stable/Clippy toolchain are still follow-ups. No live data, API signing, orders, partial-leg failures, funding, margin or speed benchmark was validated by these checks.
