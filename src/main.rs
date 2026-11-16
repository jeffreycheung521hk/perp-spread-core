//! Persistent TSV bridge for batch verification. It is not a trading daemon.
use perp_spread_core::*;
use std::io::{self, BufRead, Write};

fn levels(text: &str) -> Result<Vec<Level>> {
    if text == "-" {
        return Ok(vec![]);
    }
    text.split(',')
        .map(|item| {
            let (price, qty) = item
                .split_once('@')
                .ok_or(CoreError::Invalid("level syntax"))?;
            Level::parse(price, qty)
        })
        .collect()
}
fn snapshot(venue: Venue, bids: &str, asks: &str, received_ms: u64) -> Result<OrderBook> {
    let mut book = OrderBook::new(venue);
    book.apply(
        true,
        &levels(bids)?,
        &levels(asks)?,
        UpdateMeta {
            received_ms,
            message_id: None,
            source_ms: None,
        },
    )?;
    Ok(book)
}
fn number(s: &str) -> Result<u64> {
    s.parse().map_err(|_| CoreError::Invalid("timestamp"))
}

fn process(line: &str) -> Result<String> {
    let f: Vec<_> = line.split('\t').collect();
    if f.len() != 17 {
        return Err(CoreError::Invalid("expected 17 tab-separated fields"));
    }
    let (long_venue, short_venue) = match f[1] {
        "hyperliquid" => (Venue::Hyperliquid, Venue::Dydx),
        "dydx" => (Venue::Dydx, Venue::Hyperliquid),
        _ => return Err(CoreError::Invalid("venue")),
    };
    let long = snapshot(long_venue, f[13], f[14], number(f[11])?)?;
    let short = snapshot(short_venue, f[15], f[16], number(f[12])?)?;
    let quote = quote_pair(
        &long,
        &short,
        QuoteRequest {
            quantity: if f[2] == "auto" {
                None
            } else {
                Some(Quantity::parse(f[2])?)
            },
            budget_per_leg: Cash::parse(f[3])?,
            quantity_step: Quantity::parse(f[4])?,
            long_fee: FeeRate::parse(f[5])?,
            short_fee: FeeRate::parse(f[6])?,
        },
    )?;
    let policy = Policy {
        buffer: Bps::parse(f[8])?,
        min_net_edge: Bps::parse(f[9])?,
        ..Policy::default()
    };
    let decision = quote.assess(Bps::parse(f[7])?, number(f[10])?, policy)?;
    let immediate = quote.paper_open().mark_close(&quote)?;
    Ok([
        f[0].to_owned(),
        "OK".to_owned(),
        quote.quantity.to_string(),
        quote.entry_credit_bps.to_string(),
        quote.exit_debit_bps.to_string(),
        quote.round_trip_fee_bps.to_string(),
        quote.max_walk_slippage_bps.to_string(),
        quote.long_buy.vwap_string()?,
        quote.short_sell.vwap_string()?,
        quote.long_sell.vwap_string()?,
        quote.short_buy.vwap_string()?,
        decision.estimated_net_bps.to_string(),
        decision.eligible.to_string(),
        decision.reason.to_owned(),
        immediate.gross.to_string(),
        immediate.fees.to_string(),
        immediate.net.to_string(),
    ]
    .join("\t"))
}

fn demo() -> Result<()> {
    let input = [
        "demo",
        "hyperliquid",
        "auto",
        "1000",
        "0.001",
        "4.5",
        "5",
        "2",
        "2",
        "5",
        "1000",
        "1000",
        "1000",
        "99.99@100",
        "100.01@100",
        "100.39@100",
        "100.41@100",
    ]
    .join("\t");
    println!("Synthetic example, not historical data or an actual fill:");
    println!("{}", process(&input)?);
    let long = snapshot(Venue::Hyperliquid, "99.99@100", "100.01@100", 1000)?;
    let short = snapshot(Venue::Dydx, "100.39@100", "100.41@100", 1000)?;
    let request = QuoteRequest {
        budget_per_leg: Cash::parse("1000")?,
        quantity_step: Quantity::parse("0.001")?,
        quantity: Some(Quantity::parse("10")?),
        long_fee: FeeRate::parse("4.5")?,
        short_fee: FeeRate::parse("5")?,
    };
    let entry = quote_pair(&long, &short, request)?;
    let short_after = snapshot(Venue::Dydx, "99.99@100", "100.01@100", 4000)?;
    let exit = quote_pair(&long, &short_after, request)?;
    let pnl = entry.paper_open().mark_close(&exit)?;
    println!(
        "Hypothetical 10-SOL pair: gross USD {}, four fees USD {}, net USD {}",
        pnl.gross, pnl.fees, pnl.net
    );
    Ok(())
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--demo") {
        return Ok(demo()?);
    }
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    for line in stdin.lock().lines() {
        let line = line?;
        if line.is_empty() {
            continue;
        }
        let id = line.split('\t').next().unwrap_or("unknown");
        match process(&line) {
            Ok(result) => writeln!(stdout, "{result}")?,
            Err(error) => writeln!(stdout, "{id}\tERR\t{error}")?,
        }
        stdout.flush()?;
    }
    Ok(())
}
