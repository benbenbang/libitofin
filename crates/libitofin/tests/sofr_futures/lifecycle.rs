use super::*;

#[test]
fn pillar_dates_validation_and_failed_fixing_recovery() {
    let settings = settings(Date::new(15, Month::March, 2024));
    let index = shared(Sofr::new(Handle::empty(), settings.clone()));
    let start = Date::new(20, Month::March, 2024);
    let end = Date::new(20, Month::June, 2024);
    for (pillar, expected) in [
        (Pillar::LastRelevantDate, end),
        (Pillar::MaturityDate, end),
        (
            Pillar::CustomDate(Date::new(20, Month::April, 2024)),
            Date::new(20, Month::April, 2024),
        ),
        (Pillar::CustomDate(start), start),
        (Pillar::CustomDate(end), end),
    ] {
        let h = OvernightIndexFutureRateHelper::new(
            quote(99.0),
            start,
            end,
            &index,
            Handle::empty(),
            RateAveraging::Compound,
            pillar,
        )
        .unwrap();
        assert_eq!(h.pillar_date(), expected);
        assert_eq!(h.latest_relevant_date(), end);
    }
    for (value_date, maturity_date) in [(Date::null(), end), (start, start), (end, start)] {
        assert!(
            OvernightIndexFuture::new(
                index.clone(),
                value_date,
                maturity_date,
                Handle::empty(),
                RateAveraging::Compound
            )
            .is_err()
        );
    }
    assert!(
        OvernightIndexFutureRateHelper::new(
            Handle::empty(),
            start,
            end,
            &index,
            Handle::empty(),
            RateAveraging::Compound,
            Pillar::LastRelevantDate
        )
        .is_err()
    );
    for bad in [Date::null(), start - 1, end + 1] {
        assert!(
            OvernightIndexFutureRateHelper::new(
                quote(99.0),
                start,
                end,
                &index,
                Handle::empty(),
                RateAveraging::Compound,
                Pillar::CustomDate(bad)
            )
            .is_err()
        );
    }
    let custom = Date::new(15, Month::July, 2024);
    assert_eq!(
        SofrFutureRateHelper::new(
            quote(99.0),
            Month::June,
            2024,
            Frequency::Quarterly,
            Handle::empty(),
            Pillar::CustomDate(custom),
            settings.clone()
        )
        .unwrap()
        .pillar_date(),
        custom
    );
    for (year, freq) in [
        (2024, Frequency::Annual),
        (i32::MAX, Frequency::Quarterly),
        (2199, Frequency::Quarterly),
    ] {
        assert!(
            SofrFutureRateHelper::new(
                quote(99.0),
                Month::December,
                year,
                freq,
                Handle::empty(),
                Pillar::LastRelevantDate,
                settings.clone()
            )
            .is_err()
        );
    }
    let mut future = OvernightIndexFuture::new(
        index.clone(),
        start,
        end,
        Handle::empty(),
        RateAveraging::Simple,
    )
    .unwrap();
    settings.set_evaluation_date(end);
    assert_eq!(future.npv().unwrap(), 0.0);
    settings.set_evaluation_date(start + 1);
    assert!(
        future
            .npv()
            .unwrap_err()
            .to_string()
            .contains("missing rate")
    );
    index.add_fixing(start, 0.02).unwrap();
    assert!(
        !future
            .npv()
            .unwrap_err()
            .to_string()
            .contains("missing rate")
    );
    settings.set_evaluation_date(end + 1);
    assert_eq!(future.npv().unwrap(), 0.0);
}

#[test]
fn live_inputs_recalibrate_and_failed_inputs_recover() {
    let today = Date::new(15, Month::March, 2024);
    let settings = settings(today);
    let price = shared(SimpleQuote::new(97.0));
    let convexity = shared(SimpleQuote::new(0.001));
    let make = || {
        SofrFutureRateHelper::new(
            Handle::new(price.clone() as Shared<dyn Quote>),
            Month::June,
            2024,
            Frequency::Quarterly,
            Handle::new(convexity.clone() as Shared<dyn Quote>),
            Pillar::LastRelevantDate,
            settings.clone(),
        )
        .unwrap()
    };
    let helper = make();
    let curve = PiecewiseYieldCurve::<Discount, Linear>::new(
        today,
        vec![helper.clone() as Shared<dyn RateHelper>],
        Actual365Fixed::new(),
        Linear,
    )
    .unwrap();
    let end = helper.maturity_date();
    let initial = curve.discount_date(end, false).unwrap();
    price.set_value(96.5);
    let requoted = curve.discount_date(end, false).unwrap();
    assert!((initial - requoted).abs() > 1e-5);
    near(helper.implied_quote().unwrap(), 96.5, 1e-9);
    convexity.set_value(0.002);
    let adjusted = curve.discount_date(end, false).unwrap();
    assert!((requoted - adjusted).abs() > 1e-5);
    let fresh = PiecewiseYieldCurve::<Discount, Linear>::new(
        today,
        vec![make() as Shared<dyn RateHelper>],
        Actual365Fixed::new(),
        Linear,
    )
    .unwrap();
    near(adjusted, fresh.discount_date(end, false).unwrap(), 1e-12);
    convexity.set_value(f64::NAN);
    assert!(curve.discount_date(end, false).is_err());
    convexity.set_value(0.002);
    near(curve.discount_date(end, false).unwrap(), adjusted, 1e-12);
    settings.set_evaluation_date(Date::new(21, Month::June, 2024));
    assert!(curve.discount_date(end, false).is_err());
    let index = shared(Sofr::new(Handle::empty(), settings.clone()));
    for day in [18, 20] {
        index
            .add_fixing(Date::new(day, Month::June, 2024), 0.02)
            .unwrap();
    }
    let fixed = curve.discount_date(end, false).unwrap();
    index
        .add_fixing(Date::new(21, Month::June, 2024), 0.025)
        .unwrap();
    let today_fixed = curve.discount_date(end, false).unwrap();
    assert!((today_fixed - fixed).abs() > 1e-5);
    let fresh = PiecewiseYieldCurve::<Discount, Linear>::new(
        today,
        vec![make() as Shared<dyn RateHelper>],
        Actual365Fixed::new(),
        Linear,
    )
    .unwrap();
    near(today_fixed, fresh.discount_date(end, false).unwrap(), 1e-12);
    settings.set_evaluation_date(today);
    near(curve.discount_date(end, false).unwrap(), adjusted, 1e-12);
}
