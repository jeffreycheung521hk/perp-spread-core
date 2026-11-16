# TSV calculator protocol

Run `cargo run --locked --offline` without `--demo`. Every input record produces exactly one newline-terminated output record, with no header. Empty lines return `unknown\tERR\tempty request`. Non-UTF-8 lines return `unknown\tERR\tinvalid UTF-8 input`; subsequent valid lines are still processed. LF and CRLF are accepted, as is a final nonempty record without a newline. An empty stream contains zero records. Transport I/O failures remain fatal. This interface is for batching and cross-language verification, not low-latency order submission.

All decimal values are strings, not binary floating-point numbers. Input contains exactly 17 tab-separated fields:

| Index | Name | Example |
|---|---|---|
| 0 | Request ID | `case-1` |
| 1 | Long venue (the other is short) | `hyperliquid` or `dydx` |
| 2 | Base quantity, or auto sizing | `10` or `auto` |
| 3 | Single-leg USD reference budget; used only for `auto` | `1000` (or `0` for fixed size) |
| 4 | Quantity step | `0.001` |
| 5 | Long venue taker fee in bps | `4.5` |
| 6 | Short venue taker fee in bps | `5` |
| 7 | Previous EMA exit-price debit in bps | `2` |
| 8 | Edge buffer in bps | `2` |
| 9 | Minimum net edge in bps | `5` |
| 10 | Current local monotonic time in ms | `1000` |
| 11 | Long book local arrival time in ms | `1000` |
| 12 | Short book local arrival time in ms | `1000` |
| 13 | Long book bids | `99.99@1,99.98@5` |
| 14 | Long book asks | `100.01@1,100.03@5` |
| 15 | Short book bids | `100.39@100` |
| 16 | Short book asks | `100.41@100` |

Levels are `price@size` pairs separated by commas. `-` represents an empty side; the CLI still requires both sides to calculate a four-fill pair quote. Duplicate prices in a snapshot side are errors. Each request is a full snapshot; use the library for incremental updates or the separate two-fill `quote_exit` API. The same entry quantity step applies to both legs. Actual exchange contract/tick/lot rules must be checked by an adapter. Fixed quantity uses `Sizing::Fixed` and ignores the unused budget column; auto size still requires a valid positive budget.

Successful output has these 17 fields:

```text
id  OK  quantity  entry_credit_bps  exit_debit_bps  four_fee_bps
max_walk_slippage_bps  long_buy_vwap  short_sell_vwap
long_sell_vwap  short_buy_vwap  estimated_net_bps  eligible
reason  immediate_gross_usd  immediate_fees_usd  immediate_net_usd
```

The layout above is wrapped only for readability; the actual response is one tab-separated line. The final three values assume instant, complete opening and closing against the same books. A positive entry signal can coexist with a negative instant round-trip P&L because the signal assumes later convergence.

Errors have three fields: `id`, `ERR`, and a reason. The original ID is echoed; callers should not embed tabs or newlines in it. This local bridge has no authentication, network listener or schema-version negotiation.

The CLI's default age/skew, spread and slippage filters match `Policy::default()`. Rust library callers may supply a different `Policy`. The CLI accepts a previously estimated EMA value; it does not manage a continuous EMA history or sample clock.
