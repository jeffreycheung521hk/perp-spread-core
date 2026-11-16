//! Exact, deterministic book/cost calculations. No sockets, keys, or orders.
#![forbid(unsafe_code)]

mod book;
mod fixed;
mod strategy;

pub use book::{Fill, Level, OrderBook, Side, UpdateMeta, Venue};
pub use fixed::{Bps, Cash, FeeRate, Price, Quantity};
pub use strategy::{
    quote_pair, ClosedTrade, Decision, Ema, OpenPosition, Policy, Quote, QuoteRequest,
};

pub type Result<T> = std::result::Result<T, CoreError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreError {
    Invalid(&'static str),
    Overflow,
    MissingSnapshot,
    OutOfOrder,
    EmptyBook,
    CrossedBook,
    InsufficientDepth,
}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(reason) => write!(f, "invalid: {reason}"),
            Self::Overflow => f.write_str("arithmetic overflow"),
            Self::MissingSnapshot => f.write_str("missing snapshot"),
            Self::OutOfOrder => f.write_str("out-of-order update"),
            Self::EmptyBook => f.write_str("empty book"),
            Self::CrossedBook => f.write_str("crossed book"),
            Self::InsufficientDepth => f.write_str("insufficient top-20 depth"),
        }
    }
}
impl std::error::Error for CoreError {}
