//! Desired-behavior regressions derived from the supplied probes.rs (P1-P5/P10).
//! P7 is covered by cli.rs; P10 field writes are also compile-fail doctests.
use perp_spread_core::*;

fn level(p: &str, q: &str) -> Level {
    Level::parse(p, q).unwrap()
}
fn meta(received_ms: u64, message_id: Option<u64>, source_ms: Option<u64>) -> UpdateMeta {
    UpdateMeta {
        received_ms,
        message_id,
        source_ms,
    }
}
fn book(venue: Venue, bid: &[(&str, &str)], ask: &[(&str, &str)]) -> OrderBook {
    let mut b = OrderBook::new(venue);
    let convert = |side: &[(&str, &str)]| side.iter().map(|(p, q)| level(p, q)).collect::<Vec<_>>();
    b.apply(true, &convert(bid), &convert(ask), meta(1000, None, None))
        .unwrap();
    b
}
fn entry_request(q: &str) -> QuoteRequest {
    QuoteRequest {
        sizing: Sizing::Fixed(Quantity::parse(q).unwrap()),
        quantity_step: Quantity::parse("0.001").unwrap(),
        long_fee: FeeRate::parse("4.5").unwrap(),
        short_fee: FeeRate::parse("5").unwrap(),
    }
}
fn exit_request(q: &str) -> ExitRequest {
    ExitRequest {
        quantity: Quantity::parse(q).unwrap(),
        long_fee: FeeRate::parse("4.5").unwrap(),
        short_fee: FeeRate::parse("5").unwrap(),
    }
}

#[test]
fn p1_new_snapshot_restarts_the_message_id_sequence() {
    let mut b = OrderBook::new(Venue::Dydx);
    b.apply(
        true,
        &[level("99", "1")],
        &[level("101", "1")],
        meta(1000, Some(5000), None),
    )
    .unwrap();
    assert_eq!(
        b.apply(
            true,
            &[level("98", "1")],
            &[level("100", "1")],
            meta(2000, Some(1), None)
        ),
        Ok(UpdateOutcome::Applied)
    );
    assert_eq!(
        b.apply(false, &[level("97", "1")], &[], meta(2001, Some(2), None)),
        Ok(UpdateOutcome::Applied)
    );
    assert_eq!(
        b.best().unwrap(),
        (Price::parse("98").unwrap(), Price::parse("100").unwrap())
    );
}

#[test]
fn p1b_none_id_snapshot_also_clears_the_previous_sequence_watermark() {
    let mut b = OrderBook::new(Venue::Dydx);
    b.apply(
        true,
        &[level("99", "1")],
        &[level("101", "1")],
        meta(1000, Some(5000), None),
    )
    .unwrap();
    b.apply(
        true,
        &[level("98", "1")],
        &[level("100", "1")],
        meta(2000, None, None),
    )
    .unwrap();
    assert_eq!(
        b.apply(false, &[level("97", "1")], &[], meta(2001, Some(2), None)),
        Ok(UpdateOutcome::Applied)
    );
    assert_eq!(b.state(), BookState::Synced);
}

#[test]
fn p2_source_watermark_survives_crossing_and_explicit_invalidation() {
    let mut b = OrderBook::new(Venue::Dydx);
    b.apply(
        true,
        &[level("99", "1")],
        &[level("101", "1")],
        meta(1000, Some(10), Some(200)),
    )
    .unwrap();
    b.apply(
        false,
        &[level("102", "1")],
        &[],
        meta(1001, Some(11), Some(201)),
    )
    .unwrap();
    assert_eq!(b.state(), BookState::Crossed);
    assert_eq!(b.source_watermark_ms(), Some(201));
    b.invalidate();
    assert_eq!(b.state(), BookState::NeedsSnapshot);
    assert_eq!(b.source_watermark_ms(), Some(201));
    assert_eq!(
        b.apply(
            true,
            &[level("50", "1")],
            &[level("51", "1")],
            meta(1002, Some(1), Some(100))
        ),
        Err(CoreError::OutOfOrder)
    );
    assert_eq!(b.state(), BookState::NeedsSnapshot);
    b.apply(
        true,
        &[level("98", "1")],
        &[level("100", "1")],
        meta(1003, Some(1), Some(202)),
    )
    .unwrap();
    assert_eq!(b.state(), BookState::Synced);
}

#[test]
fn p2_stale_snapshot_cannot_replace_a_crossed_book_or_its_watermark() {
    let mut b = OrderBook::new(Venue::Dydx);
    b.apply(
        true,
        &[level("99", "1")],
        &[level("101", "1")],
        meta(1000, Some(10), Some(200)),
    )
    .unwrap();
    b.apply(
        false,
        &[level("102", "1")],
        &[],
        meta(1001, Some(11), Some(201)),
    )
    .unwrap();
    assert_eq!(
        b.apply(
            true,
            &[level("50", "1")],
            &[level("51", "1")],
            meta(1002, Some(1), Some(100))
        ),
        Err(CoreError::OutOfOrder)
    );
    assert_eq!(b.state(), BookState::Crossed);
    assert_eq!(b.meta().unwrap().received_ms, 1001);
    b.apply(
        false,
        &[level("102", "0")],
        &[],
        meta(1003, Some(12), Some(202)),
    )
    .unwrap();
    assert_eq!(b.best().unwrap().0, Price::parse("99").unwrap());
}

#[test]
fn p3_duplicate_and_older_deltas_are_no_ops_including_their_timestamps() {
    let mut b = OrderBook::new(Venue::Dydx);
    b.apply(
        true,
        &[level("99", "1")],
        &[level("101", "1")],
        meta(1000, Some(10), Some(200)),
    )
    .unwrap();
    b.apply(
        false,
        &[level("98.5", "1")],
        &[],
        meta(1001, Some(11), Some(201)),
    )
    .unwrap();
    for m in [
        meta(1002, Some(11), Some(999)),
        meta(500, Some(10), Some(100)),
    ] {
        assert_eq!(
            b.apply(false, &[level("99", "0")], &[], m),
            Ok(UpdateOutcome::Ignored)
        );
        assert_eq!(b.meta().unwrap().received_ms, 1001);
        assert_eq!(b.source_watermark_ms(), Some(201));
        assert_eq!(b.best().unwrap().0, Price::parse("99").unwrap());
    }
    b.apply(false, &[], &[], meta(1003, Some(12), Some(202)))
        .unwrap();
    assert_eq!(b.state(), BookState::Synced);
}

#[test]
fn p4_exit_does_not_require_depth_on_either_unused_entry_side() {
    let long = book(
        Venue::Hyperliquid,
        &[("99.99", "100")],
        &[("100.01", "100")],
    );
    let short = book(Venue::Dydx, &[("100.39", "100")], &[("100.41", "100")]);
    let position = quote_pair(&long, &short, entry_request("10"))
        .unwrap()
        .paper_open();
    let long_exit = book(Venue::Hyperliquid, &[("99.99", "100")], &[("100.01", "1")]);
    let short_exit = book(Venue::Dydx, &[("99.98", "1")], &[("100.01", "100")]);
    assert_eq!(
        quote_pair(&long_exit, &short_exit, entry_request("10")).unwrap_err(),
        CoreError::InsufficientDepth
    );
    let exit = quote_exit(&long_exit, &short_exit, exit_request("10")).unwrap();
    let result = position.mark_close(&exit).unwrap();
    assert_eq!(result.gross, Cash::parse("3.6").unwrap());
    assert_eq!(result.net, Cash::parse("1.698").unwrap());
}

#[test]
fn exit_can_use_one_sided_books_when_only_the_unused_sides_are_empty() {
    let long = book(Venue::Hyperliquid, &[("99.99", "100")], &[]);
    let short = book(Venue::Dydx, &[], &[("100.01", "100")]);
    assert_eq!(long.state(), BookState::Synced);
    assert_eq!(long.best(), Err(CoreError::EmptyBook));
    let exit = quote_exit(&long, &short, exit_request("10")).unwrap();
    assert_eq!(exit.long_sell().notional(), Cash::parse("999.9").unwrap());
    assert_eq!(exit.short_buy().notional(), Cash::parse("1000.1").unwrap());
}

#[test]
fn p5_fixed_sizing_has_no_budget_to_validate() {
    let long = book(
        Venue::Hyperliquid,
        &[("99.99", "100")],
        &[("100.01", "100")],
    );
    let short = book(Venue::Dydx, &[("99.99", "100")], &[("100.01", "100")]);
    let request = entry_request("10");
    let fixed = quote_pair(&long, &short, request).unwrap();
    assert_eq!(fixed.quantity(), Quantity::parse("10").unwrap());
    let invalid_auto = QuoteRequest {
        sizing: Sizing::Auto {
            budget: Cash::parse("0").unwrap(),
        },
        ..request
    };
    assert!(quote_pair(&long, &short, invalid_auto).is_err());
}

#[test]
fn p10_nine_unit_exit_cannot_be_passed_off_as_a_ten_unit_close() {
    let long = book(
        Venue::Hyperliquid,
        &[("99.99", "100")],
        &[("100.01", "100")],
    );
    let short = book(Venue::Dydx, &[("100.39", "100")], &[("100.41", "100")]);
    let entry = quote_pair(&long, &short, entry_request("10")).unwrap();
    let exit = quote_exit(&long, &short, exit_request("9")).unwrap();
    assert_eq!(exit.quantity(), Quantity::parse("9").unwrap());
    assert_eq!(exit.long_sell().quantity(), exit.quantity());
    assert_eq!(exit.short_buy().quantity(), exit.quantity());
    assert!(entry.paper_open().mark_close(&exit).is_err());
    // Writing Quote/ExitQuote/Fill fields is checked by compile-fail doctests.
}

#[test]
fn snapshot_can_start_crossed_and_uncross_using_its_next_delta() {
    let mut b = OrderBook::new(Venue::Dydx);
    b.apply(
        true,
        &[level("102", "1")],
        &[level("101", "1")],
        meta(1000, Some(1), Some(200)),
    )
    .unwrap();
    assert_eq!(b.state(), BookState::Crossed);
    assert_eq!(
        b.walk(Side::Sell, Quantity::parse("1").unwrap())
            .unwrap_err(),
        CoreError::CrossedBook
    );
    b.apply(
        false,
        &[],
        &[level("101", "0"), level("103", "1")],
        meta(1001, Some(2), Some(201)),
    )
    .unwrap();
    assert_eq!(b.state(), BookState::Synced);
    assert!(b.walk(Side::Sell, Quantity::parse("1").unwrap()).is_ok());
}

#[test]
fn explicit_invalidation_keeps_arrival_time_monotonic() {
    let mut b = book(Venue::Dydx, &[("99", "1")], &[("101", "1")]);
    b.invalidate();
    assert_eq!(
        b.apply(
            true,
            &[level("98", "1")],
            &[level("100", "1")],
            meta(999, Some(1), None)
        ),
        Err(CoreError::OutOfOrder)
    );
    assert_eq!(b.state(), BookState::NeedsSnapshot);
}

#[test]
fn p8_duplicate_snapshot_prices_are_rejected_without_overwriting_prior_depth() {
    let mut b = OrderBook::new(Venue::Dydx);
    b.apply(
        true,
        &[level("99", "1")],
        &[level("101", "1")],
        meta(1000, Some(10), Some(200)),
    )
    .unwrap();
    assert_eq!(
        b.apply(
            true,
            &[level("98", "1"), level("98", "100")],
            &[level("101", "1")],
            meta(1001, Some(1), Some(201))
        ),
        Err(CoreError::Invalid("duplicate snapshot price"))
    );
    assert_eq!(b.source_watermark_ms(), Some(200));
    assert_eq!(b.meta().unwrap().received_ms, 1000);
    assert_eq!(
        b.walk(Side::Sell, Quantity::parse("2").unwrap())
            .unwrap_err(),
        CoreError::InsufficientDepth
    );
}
