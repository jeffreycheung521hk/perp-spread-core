//! Price and size: six decimals. Cash: twelve decimals. Bps: six decimals.
use crate::{CoreError, Result};
use std::fmt;

pub(crate) const MICRO: i128 = 1_000_000;
pub(crate) const BPS_DENOMINATOR: i128 = 10_000 * MICRO;

pub(crate) fn add(a: i128, b: i128) -> Result<i128> {
    a.checked_add(b).ok_or(CoreError::Overflow)
}
pub(crate) fn sub(a: i128, b: i128) -> Result<i128> {
    a.checked_sub(b).ok_or(CoreError::Overflow)
}
pub(crate) fn mul(a: i128, b: i128) -> Result<i128> {
    a.checked_mul(b).ok_or(CoreError::Overflow)
}
pub(crate) fn floor_div(n: i128, d: i128) -> Result<i128> {
    if d <= 0 {
        return Err(CoreError::Invalid("denominator"));
    }
    Ok(n.div_euclid(d))
}
pub(crate) fn ceil_div(n: i128, d: i128) -> Result<i128> {
    let q = floor_div(n, d)?;
    if n.rem_euclid(d) == 0 {
        Ok(q)
    } else {
        add(q, 1)
    }
}

fn parse_scaled(s: &str, digits: u32) -> Result<i128> {
    let (negative, text) = match s.strip_prefix('-') {
        Some(value) => (true, value),
        None => (false, s),
    };
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || !fraction.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > digits as usize
    {
        return Err(CoreError::Invalid("decimal syntax or precision"));
    }
    let whole = whole.parse::<i128>().map_err(|_| CoreError::Overflow)?;
    let fractional = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i128>().map_err(|_| CoreError::Overflow)?
    };
    let value = add(
        mul(whole, 10_i128.pow(digits))?,
        mul(fractional, 10_i128.pow(digits - fraction.len() as u32))?,
    )?;
    if negative {
        value.checked_neg().ok_or(CoreError::Overflow)
    } else {
        Ok(value)
    }
}

pub(crate) fn display_scaled(value: i128, digits: usize) -> String {
    let magnitude = value.unsigned_abs();
    let scale = 10_u128.pow(digits as u32);
    format!(
        "{}{}.{:0width$}",
        if value < 0 { "-" } else { "" },
        magnitude / scale,
        magnitude % scale,
        width = digits
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Price(pub(crate) i128);
impl Price {
    pub fn parse(value: &str) -> Result<Self> {
        let value = parse_scaled(value, 6)?;
        if value <= 0 {
            return Err(CoreError::Invalid("non-positive price"));
        }
        Ok(Self(value))
    }
}
impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&display_scaled(self.0, 6))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(pub(crate) i128);
impl Quantity {
    pub fn parse(value: &str) -> Result<Self> {
        let value = parse_scaled(value, 6)?;
        if value < 0 {
            return Err(CoreError::Invalid("negative quantity"));
        }
        Ok(Self(value))
    }
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}
impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&display_scaled(self.0, 6))
    }
}

/// One cash atom is 10^-12 USD. Signed cash supports losses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cash(pub(crate) i128);
impl Cash {
    pub fn parse(value: &str) -> Result<Self> {
        Ok(Self(parse_scaled(value, 12)?))
    }
    pub fn atoms(self) -> i128 {
        self.0
    }
    pub fn checked_add(self, rhs: Self) -> Result<Self> {
        Ok(Self(add(self.0, rhs.0)?))
    }
    pub fn checked_sub(self, rhs: Self) -> Result<Self> {
        Ok(Self(sub(self.0, rhs.0)?))
    }
}
impl fmt::Display for Cash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&display_scaled(self.0, 12))
    }
}

/// Millionths of a basis point. One basis point is 0.01%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Bps(pub(crate) i128);
impl Bps {
    pub fn parse(value: &str) -> Result<Self> {
        Ok(Self(parse_scaled(value, 6)?))
    }
    pub fn micro_bps(self) -> i128 {
        self.0
    }
    pub fn checked_sub(self, rhs: Self) -> Result<Self> {
        Ok(Self(sub(self.0, rhs.0)?))
    }
}
impl fmt::Display for Bps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&display_scaled(self.0, 6))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FeeRate(Bps);
impl FeeRate {
    pub fn parse(value: &str) -> Result<Self> {
        let rate = Bps::parse(value)?;
        if rate.0 < 0 || rate.0 > BPS_DENOMINATOR {
            return Err(CoreError::Invalid("taker fee outside 0..10000 bps"));
        }
        Ok(Self(rate))
    }
    /// Round each fee upward by less than one cash atom; never undercharge.
    pub fn charge(self, notional: Cash) -> Result<Cash> {
        if notional.0 < 0 {
            return Err(CoreError::Invalid("negative fee notional"));
        }
        Ok(Cash(ceil_div(
            mul(notional.0, self.0 .0)?,
            BPS_DENOMINATOR,
        )?))
    }
}
