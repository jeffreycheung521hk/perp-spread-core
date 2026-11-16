use crate::fixed::{add, ceil_div, display_scaled, mul, sub, BPS_DENOMINATOR, MICRO};
use crate::{Bps, Cash, CoreError, Price, Quantity, Result};
use std::collections::BTreeMap;

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

#[derive(Debug, Clone, Copy)]
pub struct Fill {
    pub quantity: Quantity,
    pub notional: Cash,
}
impl Fill {
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

#[derive(Debug)]
pub struct OrderBook {
    venue: Venue,
    bids: BTreeMap<Price, Quantity>,
    asks: BTreeMap<Price, Quantity>,
    meta: Option<UpdateMeta>,
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
            last_source_ms: None,
            last_message_id: None,
        }
    }
    pub fn venue(&self) -> Venue {
        self.venue
    }
    pub fn invalidate(&mut self) {
        self.bids.clear();
        self.asks.clear();
        self.meta = None;
        self.last_source_ms = None;
        self.last_message_id = None;
    }
    pub fn meta(&self) -> Result<UpdateMeta> {
        self.meta.ok_or(CoreError::MissingSnapshot)
    }

    /// Zero size deletes a level. Any rejected update invalidates the old book.
    pub fn apply(
        &mut self,
        snapshot: bool,
        bids: &[Level],
        asks: &[Level],
        meta: UpdateMeta,
    ) -> Result<()> {
        let result = self.apply_inner(snapshot, bids, asks, meta);
        if result.is_err() {
            self.invalidate();
        }
        result
    }
    fn apply_inner(
        &mut self,
        snapshot: bool,
        bids: &[Level],
        asks: &[Level],
        meta: UpdateMeta,
    ) -> Result<()> {
        if !snapshot && self.meta.is_none() {
            return Err(CoreError::MissingSnapshot);
        }
        if self
            .meta
            .is_some_and(|last| meta.received_ms < last.received_ms)
            || self
                .last_message_id
                .zip(meta.message_id)
                .is_some_and(|(a, b)| b <= a)
            || self
                .last_source_ms
                .zip(meta.source_ms)
                .is_some_and(|(a, b)| b < a)
        {
            return Err(CoreError::OutOfOrder);
        }
        if snapshot {
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
        self.best()?;
        self.meta = Some(meta);
        if meta.source_ms.is_some() {
            self.last_source_ms = meta.source_ms;
        }
        if meta.message_id.is_some() {
            self.last_message_id = meta.message_id;
        }
        Ok(())
    }
    pub fn best(&self) -> Result<(Price, Price)> {
        let bid = self.bids.last_key_value().ok_or(CoreError::EmptyBook)?.0;
        let ask = self.asks.first_key_value().ok_or(CoreError::EmptyBook)?.0;
        if bid >= ask {
            return Err(CoreError::CrossedBook);
        }
        Ok((*bid, *ask))
    }
    pub fn spread_bps(&self) -> Result<Bps> {
        let (bid, ask) = self.best()?;
        Ok(Bps(ceil_div(
            mul(mul(sub(ask.0, bid.0)?, 2)?, BPS_DENOMINATOR)?,
            add(ask.0, bid.0)?,
        )?))
    }
    pub fn walk(&self, side: Side, quantity: Quantity) -> Result<Fill> {
        self.meta()?;
        self.best()?;
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
        let (bid, ask) = self.best()?;
        let best_cash = mul(
            match side {
                Side::Buy => ask.0,
                Side::Sell => bid.0,
            },
            fill.quantity.0,
        )?;
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
