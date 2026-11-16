# Review follow-ups

This pass prioritizes review #1–#4. Probe-derived regressions cover their desired behavior. Snapshot duplicate-price rejection (#9) was included because it belongs to atomic book replacement.

| Review item | State after this pass |
|---|---|
| #1 book state / reconnect / watermarks / duplicate deltas | Fixed and covered by regression tests |
| #2 exit-only quotes / irrelevant budget | Fixed with `quote_exit`, `ExitQuote`, `ExitRequest`, and `Sizing` |
| #3 writable quote fields | Fixed with private fields/getters and compile-fail doctests |
| #4 blank/non-UTF-8 CLI records | Fixed and tested through the actual binary |
| #5 EMA direction/size identity | Caller responsibility now documented; automatic keyed calibration is not implemented |
| #6 parity fixture coverage and error reasons | Still open: add cross-language fixtures for walk-slippage and venue-spread rejection; compare expected error reasons. Documentation now narrows the existing coverage claim |
| #7 Python assertions under `-O` | Still open: replace assertions with explicit exceptions and add an optimization-mode/fake-binary regression. Current checks require assertions enabled |
| #8 pinned toolchain / CI MSRV / Python 3.9 | Manual Rust 1.74.0 validation passed (46 runtime + 3 compile-fail tests). Still open: pin formatter/Clippy and add dedicated CI MSRV/Python 3.9 jobs |
| #9 duplicate snapshot prices | Fixed atomically as part of book-state work |
| #10 surplus trailing decimal zeros | Still open: exact values with extra zero digits remain rejected |
| #11 small-alpha EMA decay stall | Still open numerically; documented below |
| #12 enum rejection reasons | Still open; `Decision` retains bool/string fields |

## Small-alpha EMA limitation

Each EMA update currently rounds estimated exit cost upward to a micro-bp. When `(old - observation) * alpha_ppm < 1,000,000`, a downward update can round back to the old value. The bias can approach `1,000,000 / alpha_ppm` micro-bps: at 1 ppm this is material, while at 50,000 ppm it is about 20 micro-bps. No new alpha lower bound or high-precision accumulator is introduced in this pass. The original P6 failure was reproduced, not represented as fixed.

## Adapter decisions still required

- Measure the selected Hyperliquid feed's actual push cadence before choosing age/skew limits; the official page checked does not confirm the reported slow-feed rollout status.
- Separate connection liveness, source-data age and last book-event time. Quiet synchronized books need an explicit policy.
- Scope subscription message IDs/connection identity and detect gaps using venue-specific semantics.
- Decide whether to wait for natural uncrossing (current behavior) or implement per-level-offset uncrossing as a separate feature.
