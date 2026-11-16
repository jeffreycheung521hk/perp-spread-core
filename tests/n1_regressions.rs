//! A time-rejected delta is a missing inventory change, not a harmless replay.
use perp_spread_core::*;

fn level(price: &str, size: &str) -> Level {
    Level::parse(price, size).unwrap()
}
fn meta(received_ms: u64, id: u64, source_ms: u64) -> UpdateMeta {
    UpdateMeta {
        received_ms,
        message_id: Some(id),
        source_ms: Some(source_ms),
    }
}

#[test]
fn rejected_deletion_stops_quotes_and_later_deltas_until_a_fresh_snapshot() {
    // Source clock regression, local arrival regression, and both together.
    for rejected_meta in [meta(1001, 11, 199), meta(999, 11, 201), meta(999, 11, 199)] {
        let mut long = OrderBook::new(Venue::Hyperliquid);
        long.apply(
            true,
            &[level("99", "1")],
            &[level("101", "1")],
            meta(1000, 10, 200),
        )
        .unwrap();
        assert_eq!(
            long.apply(false, &[], &[level("101", "0")], rejected_meta),
            Err(CoreError::OutOfOrder)
        );
        assert_eq!(long.state(), BookState::NeedsSnapshot);
        assert_eq!(long.source_watermark_ms(), Some(200));
        assert_eq!(long.best(), Err(CoreError::MissingSnapshot));
        assert_eq!(
            long.walk(Side::Buy, Quantity::parse("1").unwrap())
                .unwrap_err(),
            CoreError::MissingSnapshot
        );

        // An unrelated good update must not revive the missed 101 deletion.
        assert_eq!(
            long.apply(false, &[level("98", "1")], &[], meta(1002, 12, 202)),
            Err(CoreError::MissingSnapshot)
        );
        let mut short = OrderBook::new(Venue::Dydx);
        short
            .apply(
                true,
                &[level("100", "1")],
                &[level("102", "1")],
                meta(1002, 1, 202),
            )
            .unwrap();
        let exit = quote_exit(
            &long,
            &short,
            ExitRequest {
                quantity: Quantity::parse("1").unwrap(),
                long_fee: FeeRate::parse("4.5").unwrap(),
                short_fee: FeeRate::parse("5").unwrap(),
            },
        );
        assert_eq!(exit.unwrap_err(), CoreError::MissingSnapshot);

        // Source ordering still applies while waiting for a replacement snapshot.
        assert_eq!(
            long.apply(
                true,
                &[level("50", "1")],
                &[level("51", "1")],
                meta(1003, 1, 199)
            ),
            Err(CoreError::OutOfOrder)
        );
        assert_eq!(long.state(), BookState::NeedsSnapshot);
        long.apply(
            true,
            &[level("99", "1")],
            &[level("103", "1")],
            meta(1004, 1, 203),
        )
        .unwrap();
        assert_eq!(long.state(), BookState::Synced);
        assert_eq!(
            long.walk(Side::Buy, Quantity::parse("1").unwrap())
                .unwrap()
                .notional(),
            Cash::parse("103").unwrap()
        );
        assert_eq!(
            long.apply(false, &[level("98", "1")], &[], meta(1005, 2, 204)),
            Ok(UpdateOutcome::Applied)
        );
    }
}

#[test]
fn rejected_delta_also_requires_resnapshot_when_the_previous_book_was_crossed() {
    let mut book = OrderBook::new(Venue::Dydx);
    book.apply(
        true,
        &[level("102", "1")],
        &[level("101", "1")],
        meta(1000, 10, 200),
    )
    .unwrap();
    assert_eq!(book.state(), BookState::Crossed);
    assert_eq!(
        book.apply(false, &[level("102", "0")], &[], meta(1001, 11, 199)),
        Err(CoreError::OutOfOrder)
    );
    assert_eq!(book.state(), BookState::NeedsSnapshot);
    assert_eq!(book.source_watermark_ms(), Some(200));
}
