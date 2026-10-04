//! QuantLib 1.43 compiled-oracle cases for `cashflows.cpp` spread overloads and
//! `bondfunctions.cpp` curve/yield prices. Reproduce with the High28 oracle
//! generator in `tests/fixtures/high28`. Price tolerance is 1e-10; inversion
//! tolerance is 1e-10. Values are independent of the Rust implementation.

use libitofin::cashflow::{CashFlow, Leg};
use libitofin::cashflows::{CashFlows, SimpleCashFlow};
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::interestrate::Compounding;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;

fn date(day: i32, month: Month, year: i32) -> Date {
    Date::new(day, month, year)
}

fn curve(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        date(1, Month::July, 2026),
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

fn settings() -> Shared<Settings<Date>> {
    let s = shared(Settings::new());
    s.set_evaluation_date(date(1, Month::July, 2026));
    s
}

fn leg() -> Leg {
    [
        (7.0, date(7, Month::July, 2026)),
        (20.0, date(7, Month::December, 2026)),
        (100.0, date(7, Month::July, 2027)),
    ]
    .into_iter()
    .map(|(amount, d)| shared(SimpleCashFlow::new(amount, d).unwrap()) as Shared<dyn CashFlow>)
    .collect()
}

#[test]
fn all_spread_conventions_match_compiled_quantlib_and_invert() {
    use Compounding::{Compounded, CompoundedThenSimple, Continuous, Simple, SimpleThenCompounded};
    let cases = [
        (Simple, -0.015, false, 118.33698251346071),
        (Simple, -0.015, true, 125.33756638150949),
        (Simple, 0.025, false, 114.25626219511594),
        (Simple, 0.025, true, 121.25839980501728),
        (Compounded, -0.015, false, 118.3553079435494),
        (Compounded, -0.015, true, 125.3558978530758),
        (Compounded, 0.025, false, 114.20318691680387),
        (Compounded, 0.025, true, 121.20530580297749),
        (Continuous, -0.015, false, 118.3734425191263),
        (Continuous, -0.015, true, 125.37402587676586),
        (Continuous, 0.025, false, 114.14903657951699),
        (Continuous, 0.025, true, 121.15117579521386),
        (SimpleThenCompounded, -0.015, false, 118.3550568365311),
        (SimpleThenCompounded, -0.015, true, 125.35564070457987),
        (SimpleThenCompounded, 0.025, false, 114.20393450465758),
        (SimpleThenCompounded, 0.025, true, 121.20607211455892),
        (CompoundedThenSimple, -0.015, false, 118.33723355582764),
        (CompoundedThenSimple, -0.015, true, 125.33782346535402),
        (CompoundedThenSimple, 0.025, false, 114.25551402700196),
        (CompoundedThenSimple, 0.025, true, 121.25763291317557),
    ];
    let leg = leg();
    let settings = settings();
    let handle = Handle::new(curve(0.03));
    let settlement = Some(date(7, Month::July, 2026));
    let npv_date = Some(date(9, Month::July, 2026));
    for (compounding, spread, include, expected) in cases {
        let value = CashFlows::npv_at_z_spread(
            &leg,
            handle.clone(),
            spread,
            compounding,
            Frequency::Semiannual,
            &settings,
            Some(include),
            settlement,
            npv_date,
        )
        .unwrap();
        assert!(
            (value - expected).abs() < 1e-10,
            "{compounding:?}/{spread}/{include}: {value} vs {expected}"
        );
        let recovered = CashFlows::z_spread(
            &leg,
            expected,
            handle.clone(),
            compounding,
            Frequency::Semiannual,
            &settings,
            Some(include),
            settlement,
            npv_date,
            Some(1e-12),
            Some(100),
            None,
        )
        .unwrap();
        assert!((recovered - spread).abs() < 1e-10);
    }
}

#[test]
fn spread_queries_use_relinked_curves_and_reject_invalid_inputs() {
    let leg = leg();
    let settings = settings();
    let handle = RelinkableHandle::new(curve(0.03));
    let settlement = Some(date(7, Month::July, 2026));
    let value = |spread| {
        CashFlows::npv_at_z_spread(
            &leg,
            handle.handle(),
            spread,
            Compounding::Continuous,
            Frequency::Annual,
            &settings,
            Some(false),
            settlement,
            None,
        )
    };
    let initial = value(0.01).unwrap();
    handle.link_to(curve(0.05));
    assert!(value(0.01).unwrap() < initial);
    assert!(value(f64::NAN).is_err());
    assert!(value(f64::INFINITY).is_err());
    assert!(
        CashFlows::npv_at_z_spread(
            &leg,
            Handle::empty(),
            0.0,
            Compounding::Continuous,
            Frequency::Annual,
            &settings,
            Some(false),
            settlement,
            None
        )
        .is_err()
    );
    assert!(
        CashFlows::npv_at_z_spread(
            &leg,
            handle.handle(),
            -3.0,
            Compounding::Compounded,
            Frequency::Semiannual,
            &settings,
            Some(false),
            settlement,
            None
        )
        .is_err()
    );
    for (target, accuracy, guess, limit) in [
        (f64::NAN, 1e-10, 0.0, 100),
        (100.0, f64::NAN, 0.0, 100),
        (100.0, 1e-10, f64::INFINITY, 100),
        (100.0, 1e-10, 0.0, 1),
    ] {
        assert!(
            CashFlows::z_spread(
                &leg,
                target,
                handle.handle(),
                Compounding::Continuous,
                Frequency::Annual,
                &settings,
                Some(false),
                settlement,
                None,
                Some(accuracy),
                Some(limit),
                Some(guess)
            )
            .is_err()
        );
    }
}
