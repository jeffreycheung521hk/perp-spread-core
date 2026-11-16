use crate::fixed::{add, ceil_div, floor_div, mul, sub, BPS_DENOMINATOR, MICRO};
use crate::{Bps, Cash, CoreError, FeeRate, Fill, OrderBook, Quantity, Result, Side, Venue};

#[derive(Debug, Clone, Copy)]
pub enum Sizing {
    Auto { budget: Cash },
    Fixed(Quantity),
}

#[derive(Debug, Clone, Copy)]
pub struct QuoteRequest {
    pub sizing: Sizing,
    pub quantity_step: Quantity,
    pub long_fee: FeeRate,
    pub short_fee: FeeRate,
}

/// An immutable quote; external callers can inspect but cannot rewrite its size.
///
/// ```compile_fail,E0616
/// use perp_spread_core::{Quantity, Quote};
/// fn tamper(quote: &mut Quote, new_size: Quantity) {
///     quote.quantity = new_size;
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Quote {
    long_venue: Venue,
    short_venue: Venue,
    quantity: Quantity,
    long_buy: Fill,
    short_sell: Fill,
    long_sell: Fill,
    short_buy: Fill,
    entry_fees: Cash,
    estimated_exit_fees: Cash,
    entry_credit_bps: Bps,
    exit_debit_bps: Bps,
    round_trip_fee_bps: Bps,
    max_walk_slippage_bps: Bps,
    long_spread_bps: Bps,
    short_spread_bps: Bps,
    long_received_ms: u64,
    short_received_ms: u64,
    reference_notional_times_four: i128,
}

#[derive(Debug, Clone, Copy)]
pub struct ExitRequest {
    pub quantity: Quantity,
    pub long_fee: FeeRate,
    pub short_fee: FeeRate,
}

/// Only the long venue's bids and short venue's asks enter this estimate.
///
/// ```compile_fail,E0616
/// use perp_spread_core::{ExitQuote, Quantity};
/// fn tamper(exit: &mut ExitQuote, entry_size: Quantity) {
///     exit.quantity = entry_size;
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ExitQuote {
    long_venue: Venue,
    short_venue: Venue,
    quantity: Quantity,
    long_sell: Fill,
    short_buy: Fill,
    fees: Cash,
    long_received_ms: u64,
    short_received_ms: u64,
}
impl ExitQuote {
    pub fn long_venue(&self) -> Venue {
        self.long_venue
    }
    pub fn short_venue(&self) -> Venue {
        self.short_venue
    }
    pub fn quantity(&self) -> Quantity {
        self.quantity
    }
    pub fn long_sell(&self) -> Fill {
        self.long_sell
    }
    pub fn short_buy(&self) -> Fill {
        self.short_buy
    }
    pub fn fees(&self) -> Cash {
        self.fees
    }
    pub fn long_received_ms(&self) -> u64 {
        self.long_received_ms
    }
    pub fn short_received_ms(&self) -> u64 {
        self.short_received_ms
    }
}

pub fn quote_exit(long: &OrderBook, short: &OrderBook, request: ExitRequest) -> Result<ExitQuote> {
    if long.venue() == short.venue() {
        return Err(CoreError::Invalid("same venue on both legs"));
    }
    let long_sell = long.walk(Side::Sell, request.quantity)?;
    let short_buy = short.walk(Side::Buy, request.quantity)?;
    let fees = request
        .long_fee
        .charge(long_sell.notional)?
        .checked_add(request.short_fee.charge(short_buy.notional)?)?;
    Ok(ExitQuote {
        long_venue: long.venue(),
        short_venue: short.venue(),
        quantity: request.quantity,
        long_sell,
        short_buy,
        fees,
        long_received_ms: long.meta()?.received_ms,
        short_received_ms: short.meta()?.received_ms,
    })
}

fn ratio_bps(cash: i128, reference_times_four: i128, ceil: bool) -> Result<Bps> {
    let numerator = mul(mul(cash, BPS_DENOMINATOR)?, 4)?;
    Ok(Bps(if ceil {
        ceil_div(numerator, reference_times_four)?
    } else {
        floor_div(numerator, reference_times_four)?
    }))
}

pub fn quote_pair(long: &OrderBook, short: &OrderBook, request: QuoteRequest) -> Result<Quote> {
    if long.venue() == short.venue() {
        return Err(CoreError::Invalid("same venue on both legs"));
    }
    if request.quantity_step.is_zero() {
        return Err(CoreError::Invalid("zero quantity step"));
    }
    let (lb, la) = long.best()?;
    let (sb, sa) = short.best()?;
    // Reference price is the average of the four best quotes. Keep the exact
    // denominator rather than rounding this reference to a price tick.
    let price_sum = add(add(lb.0, la.0)?, add(sb.0, sa.0)?)?;
    let quantity = match request.sizing {
        Sizing::Fixed(q) => q,
        Sizing::Auto { budget } => {
            if budget.0 <= 0 {
                return Err(CoreError::Invalid("non-positive budget"));
            }
            let raw = floor_div(mul(budget.0, 4)?, price_sum)?;
            Quantity(mul(raw / request.quantity_step.0, request.quantity_step.0)?)
        }
    };
    if quantity.is_zero() || quantity.0 % request.quantity_step.0 != 0 {
        return Err(CoreError::Invalid("zero size or size off configured step"));
    }
    let long_buy = long.walk(Side::Buy, quantity)?;
    let short_sell = short.walk(Side::Sell, quantity)?;
    let long_sell = long.walk(Side::Sell, quantity)?;
    let short_buy = short.walk(Side::Buy, quantity)?;
    let entry_fees = request
        .long_fee
        .charge(long_buy.notional)?
        .checked_add(request.short_fee.charge(short_sell.notional)?)?;
    let estimated_exit_fees = request
        .long_fee
        .charge(long_sell.notional)?
        .checked_add(request.short_fee.charge(short_buy.notional)?)?;
    let reference = mul(quantity.0, price_sum)?;
    let slips = [
        long.walk_slippage_bps(Side::Buy, long_buy)?,
        short.walk_slippage_bps(Side::Sell, short_sell)?,
        long.walk_slippage_bps(Side::Sell, long_sell)?,
        short.walk_slippage_bps(Side::Buy, short_buy)?,
    ];
    Ok(Quote {
        long_venue: long.venue(),
        short_venue: short.venue(),
        quantity,
        long_buy,
        short_sell,
        long_sell,
        short_buy,
        entry_fees,
        estimated_exit_fees,
        entry_credit_bps: ratio_bps(
            sub(short_sell.notional.0, long_buy.notional.0)?,
            reference,
            false,
        )?,
        exit_debit_bps: ratio_bps(
            sub(short_buy.notional.0, long_sell.notional.0)?,
            reference,
            true,
        )?,
        round_trip_fee_bps: ratio_bps(add(entry_fees.0, estimated_exit_fees.0)?, reference, true)?,
        max_walk_slippage_bps: slips.into_iter().max().ok_or(CoreError::EmptyBook)?,
        long_spread_bps: long.spread_bps()?,
        short_spread_bps: short.spread_bps()?,
        long_received_ms: long.meta()?.received_ms,
        short_received_ms: short.meta()?.received_ms,
        reference_notional_times_four: reference,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct Policy {
    pub min_net_edge: Bps,
    pub buffer: Bps,
    pub max_walk_slippage: Bps,
    pub max_venue_spread: Bps,
    pub max_age_ms: u64,
    pub max_pair_skew_ms: u64,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            min_net_edge: Bps(5 * MICRO),
            buffer: Bps(2 * MICRO),
            max_walk_slippage: Bps(10 * MICRO),
            max_venue_spread: Bps(20 * MICRO),
            max_age_ms: 1500,
            max_pair_skew_ms: 750,
        }
    }
}
impl Policy {
    fn validate(self) -> Result<()> {
        if [
            self.min_net_edge.0,
            self.buffer.0,
            self.max_walk_slippage.0,
            self.max_venue_spread.0,
        ]
        .iter()
        .any(|v| *v < 0)
            || self.max_age_ms == 0
        {
            return Err(CoreError::Invalid("negative threshold or zero age limit"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Decision {
    pub eligible: bool,
    pub estimated_net_bps: Bps,
    pub reason: &'static str,
}
impl Quote {
    pub fn long_venue(&self) -> Venue {
        self.long_venue
    }
    pub fn short_venue(&self) -> Venue {
        self.short_venue
    }
    pub fn quantity(&self) -> Quantity {
        self.quantity
    }
    pub fn long_buy(&self) -> Fill {
        self.long_buy
    }
    pub fn short_sell(&self) -> Fill {
        self.short_sell
    }
    pub fn long_sell(&self) -> Fill {
        self.long_sell
    }
    pub fn short_buy(&self) -> Fill {
        self.short_buy
    }
    pub fn entry_fees(&self) -> Cash {
        self.entry_fees
    }
    pub fn estimated_exit_fees(&self) -> Cash {
        self.estimated_exit_fees
    }
    pub fn entry_credit_bps(&self) -> Bps {
        self.entry_credit_bps
    }
    pub fn exit_debit_bps(&self) -> Bps {
        self.exit_debit_bps
    }
    pub fn round_trip_fee_bps(&self) -> Bps {
        self.round_trip_fee_bps
    }
    pub fn max_walk_slippage_bps(&self) -> Bps {
        self.max_walk_slippage_bps
    }
    pub fn long_spread_bps(&self) -> Bps {
        self.long_spread_bps
    }
    pub fn short_spread_bps(&self) -> Bps {
        self.short_spread_bps
    }
    pub fn long_received_ms(&self) -> u64 {
        self.long_received_ms
    }
    pub fn short_received_ms(&self) -> u64 {
        self.short_received_ms
    }
    /// Reuse an existing pair quote's exit half without re-walking either side.
    pub fn exit_quote(&self) -> ExitQuote {
        ExitQuote {
            long_venue: self.long_venue,
            short_venue: self.short_venue,
            quantity: self.quantity,
            long_sell: self.long_sell,
            short_buy: self.short_buy,
            fees: self.estimated_exit_fees,
            long_received_ms: self.long_received_ms,
            short_received_ms: self.short_received_ms,
        }
    }
    /// A signal only. It does not submit orders or guarantee future execution.
    pub fn assess(&self, previous_exit_ema: Bps, now_ms: u64, policy: Policy) -> Result<Decision> {
        policy.validate()?;
        let net = self
            .entry_credit_bps
            .checked_sub(previous_exit_ema)?
            .checked_sub(self.round_trip_fee_bps)?
            .checked_sub(policy.buffer)?;
        let reason = if self.long_received_ms > now_ms || self.short_received_ms > now_ms {
            "future_timestamp"
        } else if now_ms - self.long_received_ms > policy.max_age_ms
            || now_ms - self.short_received_ms > policy.max_age_ms
        {
            "stale_book"
        } else if self.long_received_ms.abs_diff(self.short_received_ms) > policy.max_pair_skew_ms {
            "pair_skew"
        } else if self.max_walk_slippage_bps > policy.max_walk_slippage {
            "walk_slippage"
        } else if self.long_spread_bps > policy.max_venue_spread
            || self.short_spread_bps > policy.max_venue_spread
        {
            "venue_spread"
        } else if net < policy.min_net_edge {
            "insufficient_edge"
        } else {
            "eligible"
        };
        Ok(Decision {
            eligible: reason == "eligible",
            estimated_net_bps: net,
            reason,
        })
    }
    pub fn paper_open(&self) -> OpenPosition {
        OpenPosition { entry: *self }
    }
}

/// Hypothetical fully filled pair; there is no exchange order-management state.
#[derive(Debug)]
pub struct OpenPosition {
    entry: Quote,
}
#[derive(Debug, Clone, Copy)]
pub struct ClosedTrade {
    pub gross: Cash,
    pub fees: Cash,
    pub net: Cash,
    pub net_single_leg_bps: Bps,
}
impl OpenPosition {
    /// Estimate a closing result, without mutating/closing a real account.
    pub fn mark_close(&self, exit: &ExitQuote) -> Result<ClosedTrade> {
        let entry = &self.entry;
        if exit.quantity != entry.quantity
            || exit.long_venue != entry.long_venue
            || exit.short_venue != entry.short_venue
        {
            return Err(CoreError::Invalid("closing quantity or venues differ"));
        }
        let gross = exit
            .long_sell
            .notional
            .checked_sub(entry.long_buy.notional)?
            .checked_add(
                entry
                    .short_sell
                    .notional
                    .checked_sub(exit.short_buy.notional)?,
            )?;
        // Use the supplied exit quote's fee rates: tiers may change after entry.
        let fees = entry.entry_fees.checked_add(exit.fees)?;
        let net = gross.checked_sub(fees)?;
        Ok(ClosedTrade {
            gross,
            fees,
            net,
            net_single_leg_bps: ratio_bps(net.0, entry.reference_notional_times_four, false)?,
        })
    }
}

#[derive(Debug)]
pub struct Ema {
    alpha_ppm: u32,
    warmup_samples: u64,
    count: u64,
    value: Option<Bps>,
}
impl Ema {
    pub fn new(alpha_ppm: u32, warmup_samples: u64) -> Result<Self> {
        if alpha_ppm == 0 || alpha_ppm > 1_000_000 || warmup_samples == 0 {
            return Err(CoreError::Invalid("EMA configuration"));
        }
        Ok(Self {
            alpha_ppm,
            warmup_samples,
            count: 0,
            value: None,
        })
    }
    /// Return the previous estimate, then incorporate this observation.
    /// The caller sets cadence, checks freshness and resets after a data gap.
    pub fn observe(&mut self, observation: Bps) -> Result<Option<Bps>> {
        let previous = if self.count >= self.warmup_samples {
            self.value
        } else {
            None
        };
        let alpha = i128::from(self.alpha_ppm);
        let next = match self.value {
            None => observation,
            Some(old) => Bps(ceil_div(
                add(mul(old.0, MICRO - alpha)?, mul(observation.0, alpha)?)?,
                MICRO,
            )?),
        };
        let count = self.count.checked_add(1).ok_or(CoreError::Overflow)?;
        self.value = Some(next);
        self.count = count;
        Ok(previous)
    }
    pub fn reset(&mut self) {
        self.count = 0;
        self.value = None;
    }
}
