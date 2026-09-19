use std::collections::BTreeMap;

use libitofin::cashflows::{Coupon, OvernightIndexedCoupon, RateAveraging};
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::ibor::Estr;
use libitofin::indexes::iborindex::OvernightIndex;
use libitofin::indexes::index::Index;
use libitofin::interestrate::Compounding;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

type Fields<'a> = BTreeMap<&'a str, &'a str>;

fn date(text: &str) -> Date {
    let parts: Vec<i32> = text
        .split('-')
        .map(|value| value.parse().unwrap())
        .collect();
    Date::new(parts[2], Month::from_ordinal(parts[1]), parts[0])
}

fn close(actual: f64, expected: f64, tolerance: f64, context: &str) {
    assert!(
        (actual - expected).abs() < tolerance,
        "{context}: {actual:.17} != {expected:.17}"
    );
}

struct Market {
    settings: Shared<Settings<Date>>,
    quote: Shared<SimpleQuote>,
    curve: RelinkableHandle<dyn YieldTermStructure>,
    index: Shared<OvernightIndex>,
}

impl Market {
    fn new(reference_date: Date, evaluation_date: Date, forward: f64) -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(evaluation_date);
        let quote = shared(SimpleQuote::new(forward));
        let curve: RelinkableHandle<dyn YieldTermStructure> =
            RelinkableHandle::new(shared(FlatForward::new(
                reference_date,
                Handle::new(quote.clone() as Shared<dyn Quote>),
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )));
        let index = shared(Estr::new(curve.handle(), settings.clone()));
        Self {
            settings,
            quote,
            curve,
            index,
        }
    }

    fn from_fields(fields: &Fields<'_>) -> Self {
        let market = Self::new(
            date(fields["reference_date"]),
            date(fields["evaluation_date"]),
            fields["forward_rate"].parse().unwrap(),
        );
        market
            .settings
            .set_enforces_todays_historic_fixings(fields["enforce_today"] == "true");
        for fixing in fields["fixings"]
            .split('|')
            .filter(|value| !value.is_empty())
        {
            let (day, value) = fixing.split_once(':').unwrap();
            market
                .index
                .add_fixing(date(day), value.parse().unwrap())
                .unwrap();
        }
        market
    }

    fn coupon(&self, fields: &Fields<'_>) -> OvernightIndexedCoupon {
        let averaging = match fields["averaging"] {
            "simple" => RateAveraging::Simple,
            "compound" => RateAveraging::Compound,
            other => panic!("unknown averaging {other}"),
        };
        let day_counter = match fields["coupon_day_counter"] {
            "Actual360" => Actual360::new(),
            "Actual365Fixed" => Actual365Fixed::new(),
            other => panic!("unknown day counter {other}"),
        };
        OvernightIndexedCoupon::new(
            date(fields["payment"]),
            fields["nominal"].parse().unwrap(),
            date(fields["start"]),
            date(fields["end"]),
            self.index.clone(),
            fields["gearing"].parse().unwrap(),
            fields["spread"].parse().unwrap(),
            None,
            None,
            Some(day_counter),
            averaging,
            fields["compound_spread"] == "true",
            None,
        )
        .unwrap()
    }
}

fn cases() -> Vec<Fields<'static>> {
    let mut lines = include_str!("fixtures/overnight_averaging/coupons.csv").lines();
    let columns: Vec<_> = lines.next().unwrap().split(',').collect();
    lines
        .map(|line| {
            let values: Vec<_> = line.split(',').collect();
            assert_eq!(columns.len(), values.len());
            columns.iter().copied().zip(values).collect()
        })
        .collect()
}

#[test]
fn overnight_coupons_match_quantlib_for_both_averaging_methods() {
    let cases = cases();
    assert_eq!(cases.len(), 36);
    for fields in cases {
        let context = format!("{} {}", fields["name"], fields["averaging"]);
        let market = Market::from_fields(&fields);
        let coupon = market.coupon(&fields);
        if !fields["error"].is_empty() {
            let error = coupon.rate().expect_err(&context);
            assert!(
                error.message().to_lowercase().contains("missing"),
                "{context}: {error}"
            );
            assert!(coupon.amount().is_err(), "{context}");
            continue;
        }
        close(
            coupon.rate().unwrap(),
            fields["rate"].parse().unwrap(),
            1e-12,
            &context,
        );
        close(
            coupon.amount().unwrap(),
            fields["amount"].parse().unwrap(),
            1e-8,
            &context,
        );
        for accrued in fields["accrued_amounts"].split('|') {
            let (day, expected) = accrued.split_once(':').unwrap();
            close(
                coupon.accrued_amount(date(day)).unwrap(),
                expected.parse().unwrap(),
                1e-8,
                &format!("{context} {day}"),
            );
        }
        close(
            coupon.effective_spread().unwrap(),
            fields["effective_spread"].parse().unwrap(),
            if coupon.averaging_method() == RateAveraging::Simple {
                1e-15
            } else {
                1e-12
            },
            &context,
        );
        if coupon.averaging_method() == RateAveraging::Simple {
            assert!(coupon.effective_index_fixing().is_err(), "{context}");
        } else {
            close(
                coupon.effective_index_fixing().unwrap(),
                fields["effective_index_fixing"].parse().unwrap(),
                1e-12,
                &context,
            );
        }
    }
}

#[test]
fn a_retained_simple_coupon_reads_market_and_settings_changes() {
    let market = Market::new(date("2026-07-01"), date("2026-07-01"), 0.04);
    let coupon = OvernightIndexedCoupon::new(
        date("2026-07-15"),
        1_000_000.0,
        date("2026-07-03"),
        date("2026-07-13"),
        market.index.clone(),
        1.0,
        0.0,
        None,
        None,
        None,
        RateAveraging::Simple,
        false,
        None,
    )
    .unwrap();
    let original = coupon.rate().unwrap();
    market.quote.set_value(0.08);
    assert!(coupon.rate().unwrap() > original * 1.9);
    market.quote.set_value(0.04);
    close(coupon.rate().unwrap(), original, 1e-15, "quote restored");
    market.curve.link_to(shared(FlatForward::with_rate(
        date("2026-07-01"),
        -0.01,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )));
    assert!(coupon.rate().unwrap() < 0.0);
    market.settings.set_evaluation_date(date("2026-07-06"));
    assert!(coupon.rate().unwrap_err().message().contains("Missing"));
    market.index.add_fixing(date("2026-07-03"), 0.03).unwrap();
    let forecast_today = coupon.rate().unwrap();
    market.settings.set_enforces_todays_historic_fixings(true);
    assert!(coupon.rate().is_err());
    market.index.add_fixing(date("2026-07-06"), 0.02).unwrap();
    assert!(coupon.rate().unwrap() > forecast_today);
    market.settings.set_evaluation_date(date("2026-07-07"));
    assert!(coupon.rate().is_err());
    market.index.add_fixing(date("2026-07-07"), 0.01).unwrap();
    assert!(coupon.rate().is_ok());
    market.settings.reset_evaluation_date();
    assert!(
        coupon
            .rate()
            .unwrap_err()
            .message()
            .contains("no evaluation date")
    );
}

#[test]
fn simple_history_needs_no_curve_and_missing_forecasts_return_errors() {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(date("2026-07-07"));
    let index = shared(Estr::new(Handle::empty(), settings.clone()));
    let coupon = OvernightIndexedCoupon::new(
        date("2026-07-07"),
        1_000_000.0,
        date("2026-07-03"),
        date("2026-07-06"),
        index.clone(),
        1.0,
        0.001,
        None,
        None,
        Some(Actual365Fixed::new()),
        RateAveraging::Simple,
        true,
        None,
    )
    .unwrap();
    assert!(coupon.rate().is_err());
    index.add_fixing(date("2026-07-03"), 0.03).unwrap();
    close(
        coupon.rate().unwrap(),
        0.03 * 365.0 / 360.0 + 0.001,
        1e-15,
        "history only",
    );
    settings.set_evaluation_date(date("2026-07-02"));
    assert!(coupon.rate().is_err());
}

#[test]
fn a_retained_compound_coupon_reads_quotes_fixings_and_dates() {
    let fields = cases()
        .into_iter()
        .find(|fields| {
            fields["name"] == "today_enforced_absent" && fields["averaging"] == "compound"
        })
        .unwrap();
    let market = Market::from_fields(&fields);
    let coupon = market.coupon(&fields);
    let original = coupon.rate().unwrap();
    market.quote.set_value(0.08);
    assert!(coupon.rate().unwrap() > original);
    market.quote.set_value(0.04);
    close(coupon.rate().unwrap(), original, 1e-15, "quote restored");
    market.settings.set_enforces_todays_historic_fixings(false);
    close(
        coupon.rate().unwrap(),
        original,
        1e-15,
        "enforcement disabled",
    );
    market.settings.set_enforces_todays_historic_fixings(true);
    market.index.add_fixing(date("2026-07-07"), 0.09).unwrap();
    let with_today = coupon.rate().unwrap();
    assert!(with_today > original);
    market.settings.set_evaluation_date(date("2026-07-08"));
    close(
        coupon.rate().unwrap(),
        with_today,
        1e-12,
        "today becomes history",
    );
    market.settings.set_evaluation_date(date("2026-07-09"));
    assert!(coupon.rate().unwrap_err().message().contains("Missing"));
    market.index.add_fixing(date("2026-07-08"), 0.09).unwrap();
    assert!(coupon.rate().unwrap() > with_today);
    market.curve.link_to(shared(FlatForward::with_rate(
        date("2026-07-01"),
        -0.01,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )));
    assert!(coupon.rate().unwrap() < with_today);
    market.settings.reset_evaluation_date();
    assert!(
        coupon
            .rate()
            .unwrap_err()
            .message()
            .contains("no evaluation date")
    );
}

#[test]
fn compound_partial_accrual_checks_the_full_forward_interval() {
    use libitofin::termstructures::TermStructure;
    use libitofin::termstructures::yields::DiscountCurve;

    let fields = cases()
        .into_iter()
        .find(|fields| fields["name"] == "daily_spread" && fields["averaging"] == "compound")
        .unwrap();
    let market = Market::from_fields(&fields);
    let coupon = market.coupon(&fields);
    let curve = shared(
        DiscountCurve::new(
            vec![date("2026-07-01"), date("2026-07-11")],
            vec![1.0, 0.998],
            Actual365Fixed::new(),
            None,
        )
        .unwrap(),
    );
    market.curve.link_to(curve.clone());
    assert!(coupon.accrued_amount(date("2026-07-10")).is_ok());
    assert!(coupon.accrued_amount(date("2026-07-11")).is_err());
    curve.enable_extrapolation();
    assert!(coupon.accrued_amount(date("2026-07-11")).is_ok());
    curve.disable_extrapolation();
    assert!(coupon.rate().is_err());
    market.curve.link_to(shared(FlatForward::with_rate(
        date("2026-07-08"),
        0.04,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )));
    assert!(coupon.rate().is_err());
    market.index.add_fixing(date("2026-07-07"), 0.03).unwrap();
    assert!(coupon.rate().is_ok());
}
