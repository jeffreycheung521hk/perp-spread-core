use perp_spread_core::*;

fn meta(time: u64) -> UpdateMeta {
    UpdateMeta {
        received_ms: time,
        message_id: None,
        source_ms: None,
    }
}
fn book(venue: Venue, bids: &[(&str, &str)], asks: &[(&str, &str)]) -> OrderBook {
    let mut book = OrderBook::new(venue);
    let convert = |v: &[(&str, &str)]| {
        v.iter()
            .map(|(p, q)| Level::parse(p, q).unwrap())
            .collect::<Vec<_>>()
    };
    book.apply(true, &convert(bids), &convert(asks), meta(1000))
        .unwrap();
    book
}
fn flat(venue: Venue) -> OrderBook {
    book(venue, &[("99.99", "100")], &[("100.01", "100")])
}
fn request(quantity: Option<&str>) -> QuoteRequest {
    QuoteRequest {
        budget_per_leg: Cash::parse("1000").unwrap(),
        quantity_step: Quantity::parse("0.001").unwrap(),
        quantity: quantity.map(|q| Quantity::parse(q).unwrap()),
        long_fee: FeeRate::parse("4.5").unwrap(),
        short_fee: FeeRate::parse("5").unwrap(),
    }
}

#[test]
fn walks_multiple_levels_using_exact_notional() {
    let b = book(
        Venue::Hyperliquid,
        &[("99.99", "10")],
        &[("100.01", "1"), ("100.03", "2")],
    );
    let f = b.walk(Side::Buy, Quantity::parse("3").unwrap()).unwrap();
    assert_eq!(f.notional, Cash::parse("300.07").unwrap());
    assert_eq!(f.vwap_string().unwrap(), "100.023333333333");
    assert_eq!(
        b.walk(Side::Buy, Quantity::parse("4").unwrap())
            .unwrap_err(),
        CoreError::InsufficientDepth
    );
}
#[test]
fn sell_walk_starts_at_the_highest_bid() {
    let b = book(
        Venue::Dydx,
        &[("99.97", "2"), ("99.99", "1")],
        &[("100.01", "100")],
    );
    assert_eq!(
        b.walk(Side::Sell, Quantity::parse("2").unwrap())
            .unwrap()
            .notional,
        Cash::parse("199.96").unwrap()
    );
}
#[test]
fn deltas_keep_old_levels_and_delete_zero_quantities() {
    let mut b = flat(Venue::Dydx);
    b.apply(
        false,
        &[Level::parse("99.98", "5").unwrap()],
        &[],
        meta(1001),
    )
    .unwrap();
    b.apply(
        false,
        &[Level::parse("99.99", "0").unwrap()],
        &[],
        meta(1002),
    )
    .unwrap();
    assert_eq!(b.best().unwrap().0, Price::parse("99.98").unwrap());
}
#[test]
fn snapshot_replaces_old_prices() {
    let mut b = flat(Venue::Dydx);
    b.apply(
        true,
        &[Level::parse("99", "1").unwrap()],
        &[Level::parse("101", "1").unwrap()],
        meta(1001),
    )
    .unwrap();
    assert_eq!(b.best().unwrap().0, Price::parse("99").unwrap());
}
#[test]
fn delta_before_snapshot_is_rejected() {
    assert_eq!(
        OrderBook::new(Venue::Dydx).apply(false, &[], &[], meta(0)),
        Err(CoreError::MissingSnapshot)
    );
}
#[test]
fn rejected_update_invalidates_old_book_and_requires_a_new_snapshot() {
    let mut b = flat(Venue::Dydx);
    assert_eq!(
        b.apply(false, &[Level::parse("101", "1").unwrap()], &[], meta(1001)),
        Err(CoreError::CrossedBook)
    );
    assert!(b.meta().is_err());
    assert_eq!(
        b.apply(false, &[], &[], meta(1002)),
        Err(CoreError::MissingSnapshot)
    );
    b.apply(
        true,
        &[Level::parse("99", "1").unwrap()],
        &[Level::parse("101", "1").unwrap()],
        meta(1003),
    )
    .unwrap();
    assert!(b.walk(Side::Buy, Quantity::parse("1").unwrap()).is_ok());
}
#[test]
fn duplicate_message_id_and_older_source_are_rejected() {
    for (old, new) in [
        (
            UpdateMeta {
                message_id: Some(2),
                ..meta(1000)
            },
            UpdateMeta {
                message_id: Some(2),
                ..meta(1001)
            },
        ),
        (
            UpdateMeta {
                source_ms: Some(200),
                ..meta(1000)
            },
            UpdateMeta {
                source_ms: Some(100),
                ..meta(1001)
            },
        ),
    ] {
        let mut b = OrderBook::new(Venue::Dydx);
        b.apply(
            true,
            &[Level::parse("99", "1").unwrap()],
            &[Level::parse("101", "1").unwrap()],
            old,
        )
        .unwrap();
        assert_eq!(b.apply(false, &[], &[], new), Err(CoreError::OutOfOrder));
        assert!(b.meta().is_err());
    }
}
#[test]
fn backwards_local_time_is_rejected() {
    let mut b = flat(Venue::Dydx);
    assert_eq!(
        b.apply(false, &[], &[], meta(999)),
        Err(CoreError::OutOfOrder)
    );
}
#[test]
fn source_order_check_survives_messages_with_missing_source_time() {
    let mut b = OrderBook::new(Venue::Dydx);
    b.apply(
        true,
        &[Level::parse("99", "1").unwrap()],
        &[Level::parse("101", "1").unwrap()],
        UpdateMeta {
            source_ms: Some(200),
            ..meta(1000)
        },
    )
    .unwrap();
    b.apply(false, &[], &[], meta(1001)).unwrap();
    assert_eq!(
        b.apply(
            false,
            &[],
            &[],
            UpdateMeta {
                source_ms: Some(199),
                ..meta(1002)
            }
        ),
        Err(CoreError::OutOfOrder)
    );
}
#[test]
fn only_twenty_levels_are_executable() {
    let mut b = OrderBook::new(Venue::Hyperliquid);
    let asks: Vec<_> = (100..121)
        .map(|n| Level::parse(&n.to_string(), "1").unwrap())
        .collect();
    b.apply(
        true,
        &[Level::parse("99", "100").unwrap()],
        &asks,
        meta(1000),
    )
    .unwrap();
    assert_eq!(
        b.walk(Side::Buy, Quantity::parse("21").unwrap())
            .unwrap_err(),
        CoreError::InsufficientDepth
    );
}
#[test]
fn higher_ask_alone_is_not_an_executable_edge() {
    let q = quote_pair(
        &flat(Venue::Hyperliquid),
        &book(Venue::Dydx, &[("99.99", "100")], &[("100.50", "100")]),
        request(None),
    )
    .unwrap();
    assert!(q.entry_credit_bps < Bps::parse("0").unwrap());
    assert!(
        !q.assess(Bps::parse("2").unwrap(), 1000, Policy::default())
            .unwrap()
            .eligible
    );
}
#[test]
fn four_fees_use_the_actual_fill_notionals() {
    let q = quote_pair(
        &flat(Venue::Hyperliquid),
        &flat(Venue::Dydx),
        request(Some("10")),
    )
    .unwrap();
    assert_eq!(q.round_trip_fee_bps, Bps::parse("19").unwrap());
    assert_eq!(
        q.entry_fees.checked_add(q.estimated_exit_fees).unwrap(),
        Cash::parse("1.9").unwrap()
    );
}
#[test]
fn fees_round_up_and_invalid_fee_rates_fail() {
    assert_eq!(
        FeeRate::parse("0.000001")
            .unwrap()
            .charge(Cash::parse("0.000001").unwrap())
            .unwrap(),
        Cash::parse("0.000000000001").unwrap()
    );
    assert!(FeeRate::parse("-1").is_err());
    assert!(FeeRate::parse("10001").is_err());
    assert!(FeeRate::parse("1")
        .unwrap()
        .charge(Cash::parse("-1").unwrap())
        .is_err());
}
#[test]
fn quantity_is_equal_on_both_legs_and_rounded_down() {
    let a = book(
        Venue::Hyperliquid,
        &[("102.99", "100")],
        &[("103.01", "100")],
    );
    let b = book(Venue::Dydx, &[("102.99", "100")], &[("103.01", "100")]);
    let q = quote_pair(&a, &b, request(None)).unwrap();
    assert_eq!(q.quantity, Quantity::parse("9.708").unwrap());
    assert_eq!(q.long_buy.quantity, q.short_sell.quantity);
    assert!(quote_pair(&a, &b, request(Some("0.000001"))).is_err());
}
#[test]
fn same_venue_cannot_form_a_pair() {
    assert!(quote_pair(&flat(Venue::Dydx), &flat(Venue::Dydx), request(None)).is_err());
}
#[test]
fn stale_future_and_skewed_books_cannot_trigger_entry() {
    let a = flat(Venue::Hyperliquid);
    let mut b = flat(Venue::Dydx);
    let q = quote_pair(&a, &b, request(None)).unwrap();
    assert_eq!(
        q.assess(Bps::parse("-100").unwrap(), 3000, Policy::default())
            .unwrap()
            .reason,
        "stale_book"
    );
    assert_eq!(
        q.assess(Bps::parse("-100").unwrap(), 999, Policy::default())
            .unwrap()
            .reason,
        "future_timestamp"
    );
    b.apply(false, &[], &[], meta(2000)).unwrap();
    let q = quote_pair(&a, &b, request(None)).unwrap();
    assert_eq!(
        q.assess(Bps::parse("-100").unwrap(), 2000, Policy::default())
            .unwrap()
            .reason,
        "pair_skew"
    );
}
#[test]
fn excessive_walk_slippage_rejects_an_otherwise_large_edge() {
    let a = book(
        Venue::Hyperliquid,
        &[("99.99", "100")],
        &[("100.01", "1"), ("100.30", "99")],
    );
    let b = book(Venue::Dydx, &[("100.49", "100")], &[("100.51", "100")]);
    let q = quote_pair(&a, &b, request(Some("10"))).unwrap();
    assert_eq!(
        q.assess(Bps::parse("2").unwrap(), 1000, Policy::default())
            .unwrap()
            .reason,
        "walk_slippage"
    );
}
#[test]
fn wide_venue_spread_is_rejected() {
    let q = quote_pair(
        &flat(Venue::Hyperliquid),
        &book(Venue::Dydx, &[("100.39", "100")], &[("101", "100")]),
        request(None),
    )
    .unwrap();
    assert_eq!(
        q.assess(Bps::parse("-100").unwrap(), 1000, Policy::default())
            .unwrap()
            .reason,
        "venue_spread"
    );
}
#[test]
fn enough_edge_produces_a_signal() {
    let q = quote_pair(
        &flat(Venue::Hyperliquid),
        &book(Venue::Dydx, &[("100.39", "100")], &[("100.41", "100")]),
        request(None),
    )
    .unwrap();
    assert!(
        q.assess(Bps::parse("2").unwrap(), 1000, Policy::default())
            .unwrap()
            .eligible
    );
}
#[test]
fn ema_uses_previous_samples_and_resets_on_request() {
    let mut ema = Ema::new(50_000, 2).unwrap();
    assert_eq!(ema.observe(Bps::parse("2").unwrap()).unwrap(), None);
    assert_eq!(ema.observe(Bps::parse("2").unwrap()).unwrap(), None);
    assert_eq!(
        ema.observe(Bps::parse("100").unwrap()).unwrap(),
        Some(Bps::parse("2").unwrap())
    );
    ema.reset();
    assert_eq!(ema.observe(Bps::parse("2").unwrap()).unwrap(), None);
}
#[test]
fn signed_ema_rounding_is_conservative() {
    let mut ema = Ema::new(500_000, 1).unwrap();
    ema.observe(Bps::parse("-0.000001").unwrap()).unwrap();
    ema.observe(Bps::parse("0").unwrap()).unwrap();
    assert_eq!(
        ema.observe(Bps::parse("0").unwrap()).unwrap(),
        Some(Bps::parse("0").unwrap())
    );
}
#[test]
fn closing_trade_reconciles_gross_fees_and_net() {
    let a = flat(Venue::Hyperliquid);
    let b = book(Venue::Dydx, &[("100.39", "100")], &[("100.41", "100")]);
    let entry = quote_pair(&a, &b, request(Some("10"))).unwrap();
    let exit = quote_pair(&a, &flat(Venue::Dydx), request(Some("10"))).unwrap();
    let result = entry.paper_open().mark_close(&exit).unwrap();
    assert_eq!(result.gross, Cash::parse("3.6").unwrap());
    assert_eq!(result.fees, Cash::parse("1.902").unwrap());
    assert_eq!(result.net, Cash::parse("1.698").unwrap());
    assert_eq!(result.gross.checked_sub(result.fees).unwrap(), result.net);
}
#[test]
fn closing_fee_rate_can_change_after_entry() {
    let a = flat(Venue::Hyperliquid);
    let b = flat(Venue::Dydx);
    let entry = quote_pair(&a, &b, request(Some("10"))).unwrap();
    let exit = quote_pair(
        &a,
        &b,
        QuoteRequest {
            long_fee: FeeRate::parse("1").unwrap(),
            short_fee: FeeRate::parse("1").unwrap(),
            ..request(Some("10"))
        },
    )
    .unwrap();
    let result = entry.paper_open().mark_close(&exit).unwrap();
    assert_eq!(result.fees, Cash::parse("1.149995").unwrap());
}
#[test]
fn instant_round_trip_loses_spread_and_fees() {
    let q = quote_pair(
        &flat(Venue::Hyperliquid),
        &flat(Venue::Dydx),
        request(Some("10")),
    )
    .unwrap();
    assert_eq!(
        q.paper_open().mark_close(&q).unwrap().net,
        Cash::parse("-2.3").unwrap()
    );
}
#[test]
fn closing_a_different_quantity_or_direction_fails() {
    let a = flat(Venue::Hyperliquid);
    let b = flat(Venue::Dydx);
    let entry = quote_pair(&a, &b, request(Some("10"))).unwrap();
    assert!(entry
        .paper_open()
        .mark_close(&quote_pair(&a, &b, request(Some("9"))).unwrap())
        .is_err());
    assert!(entry
        .paper_open()
        .mark_close(&quote_pair(&b, &a, request(Some("10"))).unwrap())
        .is_err());
}
#[test]
fn invalid_inputs_are_rejected_without_silent_rounding() {
    for invalid in ["NaN", "Infinity", "1e2", "1.0000001", "-1", "0"] {
        assert!(Price::parse(invalid).is_err(), "{invalid}");
    }
    assert!(Quantity::parse("-1").is_err());
    assert!(Ema::new(0, 30).is_err());
    assert!(Ema::new(1, 0).is_err());
}
#[test]
fn zero_size_and_invalid_fill_display_are_errors() {
    assert!(flat(Venue::Dydx)
        .walk(Side::Buy, Quantity::parse("0").unwrap())
        .is_err());
    let fill = Fill {
        quantity: Quantity::parse("0").unwrap(),
        notional: Cash::parse("0").unwrap(),
    };
    assert!(fill.vwap_string().is_err());
}
#[test]
fn arithmetic_overflow_returns_error_instead_of_wrapping() {
    let huge = "1000000000000000000000000";
    let b = book(
        Venue::Hyperliquid,
        &[(huge, "100")],
        &[("2000000000000000000000000", "100")],
    );
    assert_eq!(
        b.walk(Side::Buy, Quantity::parse(huge).unwrap())
            .unwrap_err(),
        CoreError::Overflow
    );
}
