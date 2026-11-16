# Review fixes: book state, exit quotes and byte framing

These changes intentionally adjust the v0.1 Rust API before an exchange adapter is added. The 17-field successful TSV request/response layout is unchanged. MIT licensing is a separate preceding commit on `main`.

## Book state and ordering

`OrderBook::apply` now returns `Result<UpdateOutcome>`:

| Event | Result | Depth and quoting |
|---|---|---|
| Initial or validated replacement snapshot | `Applied` | Replaces depth; starts a new message-ID sequence |
| Valid incremental update | `Applied` | Updates depth and recalculates state |
| Delta with ID <= current sequence watermark | `Ignored` | No depth, timestamp or watermark changes |
| Crossed snapshot/delta | `Applied`, state `Crossed` | Retains depth; quote methods return `CrossedBook` |
| Later delta restores bid < ask | `Applied`, state `Synced` | Quotes become available again if depth is sufficient |
| Snapshot with backward source/arrival timestamp | `Err(OutOfOrder)` | Last accepted depth, state and watermarks are unchanged |
| New delta with backward source/arrival timestamp | `Err(OutOfOrder)` | State becomes `NeedsSnapshot`; depth is cleared, time watermarks are retained |
| Duplicate snapshot price on the same side | `Err(Invalid(...))` | Rejected atomically; last accepted state is unchanged |
| Explicit `invalidate()` | State `NeedsSnapshot` | Clears depth/metadata and ends the ID sequence; retains time watermarks |

`Synced` describes the snapshot/delta baseline, not guaranteed liquidity. An accepted snapshot may have an empty side. `best()` needs both sides; `walk` and `best_price` need only their executable side. All of them reject `Crossed` and `NeedsSnapshot` states.

A validated snapshot resets the message-ID watermark to that snapshot's ID, **including `None`**. Source and local-arrival time watermarks survive both snapshots and invalidation. The source guard only applies when the incoming message includes a source timestamp; local arrival time alone cannot establish source freshness.

A time-rejected delta may contain an inventory change that was not applied. The book must therefore stop quoting and reject later deltas until a valid replacement snapshot arrives. A rejected snapshot does not create that incremental gap and leaves the existing book untouched. Duplicate/backward delta IDs are checked first and remain harmless ignored replays; they do not invalidate the book or extend freshness. Regression tests reproduce a rejected `101@0` deletion and ensure the old `101@1` ask cannot subsequently be used for a quote or close estimate.

Adapters must call `invalidate()` when a connection is retired or a data gap is detected and must keep late messages from old connection IDs out of the new sequence. Construct a new `OrderBook` only when deliberately changing instrument or clock domain. The core does not detect all sequence gaps and does not implement a per-price offset-based artificial uncrossing algorithm. It waits for subsequent accepted updates to restore an uncrossed book.

Official dYdX documentation describes crossed books that may naturally uncross, initial subscription snapshots with incremental updates, and a connection handshake whose message ID starts at zero:

- [Watch orderbook and uncrossing](https://docs.dydx.xyz/interaction/data/watch-orderbook)
- [WebSocket guide and connection handshake](https://docs.dydx.xyz/interaction/data/feeds)
- [WebSocket message schemas](https://docs.dydx.xyz/indexer-client/websockets)

## Fixed sizing and two-fill exits

Replace the old independent `quantity: Option<Quantity>` and `budget_per_leg` fields with:

```rust
use perp_spread_core::{Cash, Quantity, Sizing};

let auto = Sizing::Auto { budget: Cash::parse("1000").unwrap() };
let fixed = Sizing::Fixed(Quantity::parse("10").unwrap());
```

`QuoteRequest` still carries the entry quantity step and the two fees. Only automatic sizing requires a positive budget. Fixed size has no unused budget to validate.

For an existing pair, use `quote_exit(long_book, short_book, ExitRequest { quantity, long_fee, short_fee })`. It walks just long bids and short asks. It neither inspects unused entry-side depth nor requires an entry sizing budget/step. It still rejects crossed/uninitialized books and insufficient depth on a **required** side.

`OpenPosition::mark_close` now accepts `&ExitQuote`. If an existing four-fill `Quote` already exists, `quote.exit_quote()` extracts its exit half. Prefer `quote_exit` when estimating a new closing snapshot so irrelevant entry depth cannot block the estimate.

## Immutable values

Replace field reads such as `quote.quantity` with `quote.quantity()`, and `quote.long_buy.notional` with `quote.long_buy().notional()`. Getter results are values, not mutable references into the quote. `Quote`, `ExitQuote` and `Fill` cannot be constructed or rewritten through public financial fields. Quantity/direction mismatch checks therefore cannot be bypassed by rewriting a displayed size. Compile-fail doctests protect these API boundaries.

## TSV byte framing

The bridge reads bytes through `read_until(b'\n')` and decodes each record independently:

- Blank record: one `unknown / ERR / empty request` response.
- Invalid UTF-8 record: one `unknown / ERR / invalid UTF-8 input` response, then continue.
- LF, CRLF and a final nonempty record without a newline are accepted.
- Empty byte stream: no records and no responses.
- Fixed size ignores the reserved budget column; auto size validates it.
- Actual stdin/stdout transport failures remain fatal.

## Feed cadence, liveness and EMA identity

Keep an EMA per instrument, venue direction and size policy. Never pool opposite-direction exit costs. Changing size materially can change walked costs; reset or maintain a separate calibration.

`received_ms` is the time of the last applied book event, not a transport heartbeat. An ignored duplicate cannot extend freshness. Do not manufacture empty deltas from a generic ping merely to pass the current age filter. An adapter needs separate liveness, source-age and quiet-book policies.

The [Hyperliquid subscription documentation](https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions), checked on 2026-11-16, documents 5 levels for `fast` and 20 for slow L2 books. Its type comment says snapshots are pushed on blocks at least "0.5 since last push", without explicitly stating a time unit. That page does **not** establish the rollout status of the reported 2-second/5-second slow-feed changes. Actual venue cadence remains unverified here. The existing 1500 ms age / 750 ms skew defaults remain illustrative and were not silently retuned from a third-party announcement.
