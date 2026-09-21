use libitofin::currency::Currency;
use libitofin::handle::Handle;
use libitofin::indexes::iborindex::IborIndex;
use libitofin::math::interpolations::linear::Linear;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::bootstraptraits::ZeroYield;
use libitofin::termstructures::iterativebootstrap::{
    IterativeBootstrap, IterativeBootstrapOptions,
};
use libitofin::termstructures::yields::{DepositRateHelper, PiecewiseYieldCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;

type Curve = PiecewiseYieldCurve<ZeroYield, Linear>;

fn build(
    rate: f64,
    options: IterativeBootstrapOptions,
) -> (Shared<Curve>, Shared<SimpleQuote>, Shared<Settings<Date>>) {
    let today = Date::new(15, Month::June, 2026);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    let index = IborIndex::new(
        "robustness".to_owned(),
        Period::new(1, TimeUnit::Years),
        0,
        Currency::eur(),
        NullCalendar::new(),
        BusinessDayConvention::Unadjusted,
        false,
        Actual365Fixed::new(),
        Handle::empty(),
        settings.clone(),
    );
    let quote = shared(SimpleQuote::new(rate));
    let helper = DepositRateHelper::new(Handle::new(quote.clone() as Shared<dyn Quote>), &index);
    let curve = Curve::with_bootstrap(
        today,
        vec![helper],
        Actual365Fixed::new(),
        Linear,
        IterativeBootstrap::with_options(options).unwrap(),
    )
    .unwrap();
    (curve, quote, settings)
}

fn near(actual: f64, expected: f64) {
    assert!(
        actual.is_finite() && (actual - expected).abs() <= 1e-12,
        "{actual} != {expected}"
    );
}

#[test]
fn quantlib_sign_aware_widening_and_attempt_exhaustion() {
    for (rate, min, max, expected) in [
        (0.015, 0.04, 0.1, 0.014888612493543705),
        (-0.015, -0.1, -0.04, -0.015113637809893627),
        (0.25, 0.01, 0.1, 0.22314355131420974),
        (-0.25, -0.1, -0.01, -0.2876820724517808),
    ] {
        let mut options = IterativeBootstrapOptions {
            min_value: Some(min),
            max_value: Some(max),
            max_attempts: 2,
            ..Default::default()
        };
        let (curve, quote, _) = build(rate, options);
        assert!(
            curve
                .data()
                .unwrap_err()
                .message()
                .contains("root not bracketed")
        );
        quote.set_value(if rate > 0.0 { 0.05 } else { -0.05 });
        near(
            curve.discount(1.0, true).unwrap(),
            1.0 / (1.0 + if rate > 0.0 { 0.05 } else { -0.05 }),
        );
        options.max_attempts = 3;
        let (curve, _, _) = build(rate, options);
        near(curve.data().unwrap()[1], expected);
        near(curve.discount(1.0, true).unwrap(), 1.0 / (1.0 + rate));
    }
}

#[test]
fn asymmetric_widening_factors_are_applied_to_the_correct_bound() {
    for (rate, min, max, min_factor, max_factor) in
        [(0.25, 0.01, 0.1, 1.0, 3.0), (-0.25, -0.1, -0.01, 3.0, 1.0)]
    {
        let options = IterativeBootstrapOptions {
            min_value: Some(min),
            max_value: Some(max),
            max_attempts: 2,
            min_factor,
            max_factor,
            ..Default::default()
        };
        let (curve, _, _) = build(rate, options);
        near(curve.discount(1.0, true).unwrap(), 1.0 / (1.0 + rate));
    }
}

#[test]
fn quantlib_inclusive_fallback_scan_preserves_node_zero_semantics() {
    for (rate, min, max, evaluations, first, pillar, discount) in [
        (
            0.25,
            0.01,
            0.1,
            100,
            0.10000000000000003,
            0.10000000000000003,
            0.9048374180359595,
        ),
        (
            -0.25,
            -0.1,
            -0.01,
            100,
            -0.009999999999999967,
            -0.1,
            1.1051709180756477,
        ),
        (
            0.25,
            0.01,
            0.4,
            1,
            0.39999999999999997,
            0.20500000000000002,
            0.8146473164114145,
        ),
    ] {
        let options = IterativeBootstrapOptions {
            min_value: Some(min),
            max_value: Some(max),
            dont_throw: true,
            max_evaluations: evaluations,
            ..Default::default()
        };
        let (curve, quote, _) = build(rate, options);
        let data = curve.data().unwrap();
        near(data[0], first);
        near(data[1], pillar);
        near(curve.discount(1.0, true).unwrap(), discount);
        for invalid in [None, Some(f64::NAN), Some(f64::INFINITY)] {
            quote.set_value(invalid);
            assert!(curve.data().is_err());
            quote.set_value(rate);
            near(curve.discount(1.0, true).unwrap(), discount);
        }
    }
}

#[test]
fn cached_curve_is_discarded_once_before_strict_fresh_retry() {
    let (curve, quote, settings) = build(0.01, IterativeBootstrapOptions::default());
    near(curve.data().unwrap()[1], 0.009950330853168134);
    quote.set_value(0.4);
    near(curve.data().unwrap()[1], 0.33647223662143483);
    let (fresh, _, _) = build(0.4, IterativeBootstrapOptions::default());
    near(
        curve.discount(0.5, true).unwrap(),
        fresh.discount(0.5, true).unwrap(),
    );
    quote.set_value(10.0);
    assert!(curve.data().is_err());
    quote.set_value(0.01);
    near(curve.data().unwrap()[1], 0.009950330853168134);
    settings.set_evaluation_date(Date::new(16, Month::June, 2026));
    let maturity = Date::new(16, Month::June, 2027);
    near(
        curve.discount_date(maturity, true).unwrap()
            / curve
                .discount_date(Date::new(16, Month::June, 2026), true)
                .unwrap(),
        1.0 / 1.01,
    );
}

#[test]
fn malformed_options_are_rejected_before_curve_construction() {
    let base = IterativeBootstrapOptions::default();
    for options in [
        IterativeBootstrapOptions {
            accuracy: Some(0.0),
            ..base
        },
        IterativeBootstrapOptions {
            accuracy: Some(f64::NAN),
            ..base
        },
        IterativeBootstrapOptions {
            min_value: Some(f64::NEG_INFINITY),
            ..base
        },
        IterativeBootstrapOptions {
            max_value: Some(f64::INFINITY),
            ..base
        },
        IterativeBootstrapOptions {
            min_value: Some(1.0),
            max_value: Some(1.0),
            ..base
        },
        IterativeBootstrapOptions {
            max_attempts: 0,
            ..base
        },
        IterativeBootstrapOptions {
            dont_throw_steps: 0,
            ..base
        },
        IterativeBootstrapOptions {
            max_evaluations: 0,
            ..base
        },
        IterativeBootstrapOptions {
            min_factor: 0.9,
            ..base
        },
        IterativeBootstrapOptions {
            max_factor: f64::NAN,
            ..base
        },
    ] {
        assert!(IterativeBootstrap::with_options(options).is_err());
    }
    assert!(IterativeBootstrap::with_options(base).is_ok());
}
