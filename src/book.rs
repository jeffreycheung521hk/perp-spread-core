use crate::fixed::{add, ceil_div, display_scaled, mul, sub, BPS_DENOMINATOR, MICRO};
use crate::{Bps, Cash, CoreError, Price, Quantity, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Venue {
    Hyperliquid,
    Dydx,
}
#[derive(Debug, Clone, Copy)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy)]
pub struct Level {
    pub price: Price,
    pub quantity: Quantity,
}
impl Level {
    pub fn parse(price: &str, quantity: &str) -> Result<Self> {
        Ok(Self {
            price: Price::parse(price)?,
            quantity: Quantity::parse(quantity)?,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UpdateMeta {
    /// Monotonic local arrival time, using the SAME clock for both venues.
    pub received_ms: u64,
    pub message_id: Option<u64>,
    pub source_ms: Option<u64>,
}

/// Immutable walked fill. It can only be produced by the book walker.
///
/// ```compile_fail,E0616
/// use perp_spread_core::{Cash, Fill};
/// fn tamper(fill: &mut Fill, fake_notional: Cash) {
///     fill.notional = fake_notional;
/// }
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Fill {
    pub(crate) quantity: Quantity,
    pub(crate) notional: Cash,
}
impl Fill {
    pub fn quantity(self) -> Quantity {
        self.quantity
    }
    pub fn notional(self) -> Cash {
        self.notional
    }
    /// Display only: accounting keeps the exact notional.
    pub fn vwap_string(self) -> Result<String> {
        if self.quantity.is_zero() || self.notional.0 <= 0 {
            return Err(CoreError::Invalid("non-positive fill"));
        }
        Ok(display_scaled(
            mul(self.notional.0, MICRO)? / self.quantity.0,
            12,
        ))
    }
}

/// Stream synchronization is separate from the availability of executable depth.
/// `Synced` may have an empty side; the relevant quote/walk then returns EmptyBook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookState {
    NeedsSnapshot,
    Synced,
    Crossed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateOutcome {
    Applied,
    Ignored,
}

#[derive(Debug)]
pub struct OrderBook {
    venue: Venue,
    bids: BTreeMap<Price, Quantity>,
    asks: BTreeMap<Price, Quantity>,
    meta: Option<UpdateMeta>,
    state: BookState,
    last_received_ms: Option<u64>,
    last_source_ms: Option<u64>,
    last_message_id: Option<u64>,
}
impl OrderBook {
    pub fn new(venue: Venue) -> Self {
        Self {
            venue,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            meta: None,
            state: BookState::NeedsSnapshot,
            last_received_ms: None,
            last_source_ms: None,
            last_message_id: None,
        }
    }
    pub fn venue(&self) -> Venue {
        self.venue
    }
    pub fn state(&self) -> BookState {
        self.state
    }
    pub fn source_watermark_ms(&self) -> Option<u64> {
        self.last_source_ms
    }
    /// Discard depth and end the sequence, but retain source/arrival watermarks.
    /// Construct a new book only when intentionally changing clock/instrument.
    pub fn invalidate(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.meta = None;
        self.state = BookState::NeedsSnapshot;
        self.last_message_id = None;
    }
    pub fn meta(&self) -> Result<UpdateMeta> {
        self.meta.ok_or(CoreError::MissingSnapshot)
    }

    /// A validated snapshot starts a new sequence, even when its ID is None.
    /// Old/duplicate delta IDs are ignored without refreshing timestamps.
    /// Crossed depth is retained; quote methods stay disabled until it uncrosses.
    /// Rejected snapshots preserve the last accepted book. Time-rejected deltas
    /// require a new snapshot because their depth changes may have been missed.
    /// Source/arrival watermarks survive that invalidation.
    pub fn apply(
        &mut self,
        snapshot: bool,
        bids: &[Level],
        asks: &[Level],
        meta: UpdateMeta,
    ) -> Result<UpdateOutcome> {
        if !snapshot && self.state == BookState::NeedsSnapshot {
            return Err(CoreError::MissingSnapshot);
        }
        if !snapshot
            && self
                .last_message_id
                .zip(meta.message_id)
                .is_some_and(|(a, b)| b <= a)
        {
            return Ok(UpdateOutcome::Ignored);
        }
        if self
            .last_received_ms
            .is_some_and(|last| meta.received_ms < last)
            || self
                .last_source_ms
                .zip(meta.source_ms)
                .is_some_and(|(a, b)| b < a)
        {
            if !snapshot {
                self.invalidate();
            }
            return Err(CoreError::OutOfOrder);
        }
        if snapshot {
            validate_snapshot_levels(bids)?;
            validate_snapshot_levels(asks)?;
            self.bids.clear();
            self.asks.clear();
        }
        for (target, changes) in [(&mut self.bids, bids), (&mut self.asks, asks)] {
            for level in changes {
                if level.quantity.is_zero() {
                    target.remove(&level.price);
                } else {
                    target.insert(level.price, level.quantity);
                }
            }
        }
        self.meta = Some(meta);
        self.last_received_ms = Some(meta.received_ms);
        if meta.source_ms.is_some() {
            self.last_source_ms = meta.source_ms;
        }
        if snapshot || meta.message_id.is_some() {
            self.last_message_id = meta.message_id;
        }
        self.state = match (self.bids.last_key_value(), self.asks.first_key_value()) {
            (Some((bid, _)), Some((ask, _))) if bid >= ask => BookState::Crossed,
            _ => BookState::Synced,
        };
        Ok(UpdateOutcome::Applied)
    }
    fn ensure_quotable(&self) -> Result<()> {
        match self.state {
            BookState::NeedsSnapshot => Err(CoreError::MissingSnapshot),
            BookState::Crossed => Err(CoreError::CrossedBook),
            BookState::Synced => Ok(()),
        }
    }
    /// Best price for just the executable side; the unused side may be empty.
    pub fn best_price(&self, side: Side) -> Result<Price> {
        self.ensure_quotable()?;
        let level = match side {
            Side::Buy => self.asks.first_key_value(),
            Side::Sell => self.bids.last_key_value(),
        };
        level.map(|(price, _)| *price).ok_or(CoreError::EmptyBook)
    }
    pub fn best(&self) -> Result<(Price, Price)> {
        Ok((self.best_price(Side::Sell)?, self.best_price(Side::Buy)?))
    }
    pub fn spread_bps(&self) -> Result<Bps> {
        let (bid, ask) = self.best()?;
        Ok(Bps(ceil_div(
            mul(mul(sub(ask.0, bid.0)?, 2)?, BPS_DENOMINATOR)?,
            add(ask.0, bid.0)?,
        )?))
    }
    pub fn walk(&self, side: Side, quantity: Quantity) -> Result<Fill> {
        self.best_price(side)?;
        if quantity.is_zero() {
            return Err(CoreError::Invalid("zero trade size"));
        }
        match side {
            Side::Buy => walk_levels(self.asks.iter().take(20), quantity),
            Side::Sell => walk_levels(self.bids.iter().rev().take(20), quantity),
        }
    }
    pub fn walk_slippage_bps(&self, side: Side, fill: Fill) -> Result<Bps> {
        if fill.quantity.is_zero() || fill.notional.0 <= 0 {
            return Err(CoreError::Invalid("non-positive fill"));
        }
        let best_cash = mul(self.best_price(side)?.0, fill.quantity.0)?;
        let cost = match side {
            Side::Buy => sub(fill.notional.0, best_cash)?,
            Side::Sell => sub(best_cash, fill.notional.0)?,
        };
        if cost < 0 {
            return Err(CoreError::Invalid("fill does not match book side"));
        }
        Ok(Bps(ceil_div(mul(cost, BPS_DENOMINATOR)?, best_cash)?))
    }
}

fn validate_snapshot_levels(levels: &[Level]) -> Result<()> {
    let mut seen = BTreeSet::new();
    for level in levels {
        if !seen.insert(level.price) {
            return Err(CoreError::Invalid("duplicate snapshot price"));
        }
    }
    Ok(())
}

fn walk_levels<'a>(
    levels: impl Iterator<Item = (&'a Price, &'a Quantity)>,
    quantity: Quantity,
) -> Result<Fill> {
    let mut remaining = quantity.0;
    let mut notional = 0_i128;
    for (price, available) in levels {
        let amount = remaining.min(available.0);
        notional = add(notional, mul(price.0, amount)?)?;
        remaining = sub(remaining, amount)?;
        if remaining == 0 {
            return Ok(Fill {
                quantity,
                notional: Cash(notional),
            });
        }
    }
    Err(CoreError::InsufficientDepth)
}
