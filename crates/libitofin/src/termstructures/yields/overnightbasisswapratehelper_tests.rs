use super::*;
use crate::indexes::ibor::{estr::Estr, euribor::Euribor};
use crate::interestrate::Compounding;
use crate::math::interpolations::loglinear::LogLinear;
use crate::quotes::SimpleQuote;
use crate::termstructures::bootstraptraits::Discount;
use crate::termstructures::yields::{FlatForward, PiecewiseYieldCurve};
use crate::test_support::{Flag, as_observer};
use crate::time::calendars::target::Target;
use crate::time::date::Month;
use crate::time::daycounters::actual360::Actual360;
use crate::time::frequency::Frequency;

#[test]
fn live_inputs_fixings_dates_and_owner_lifetime() {
    let settings = shared(Settings::<Date>::new());
    let today = Date::new(23, Month::October, 2025);
    settings.set_evaluation_date(today);
    let overnight_rate = shared(SimpleQuote::new(0.021));
    let discount_rate = shared(SimpleQuote::new(0.012));
    let curve = |q: &Shared<SimpleQuote>| {
        Handle::new(shared(FlatForward::new(
            today,
            Handle::new(q.clone() as Shared<dyn Quote>),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let overnight = shared(Estr::new(curve(&overnight_rate), settings.clone()));
    let ibor = shared(Euribor::three_months(Handle::empty(), settings.clone()));
    let h = OvernightIborBasisSwapRateHelper::new(
        Handle::new(shared(SimpleQuote::new(0.002)) as Shared<dyn Quote>),
        Period::new(5, TimeUnit::Years),
        2,
        Target::new(),
        BusinessDayConvention::ModifiedFollowing,
        false,
        &overnight,
        &ibor,
        curve(&discount_rate),
    )
    .unwrap();
    let fitted = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        today,
        vec![h.clone() as Shared<dyn RateHelper>],
        Actual360::new(),
        LogLinear,
    )
    .unwrap();
    let mut previous = fitted.discount_date(h.pillar_date(), false).unwrap();
    let flag = Flag::new();
    h.observable().register_observer(&as_observer(&flag));
    for (q, value) in [(&overnight_rate, 0.035), (&discount_rate, 0.06)] {
        Flag::lower(&flag);
        q.set_value(value);
        assert!(Flag::is_up(&flag));
        let next = fitted.discount_date(h.pillar_date(), false).unwrap();
        assert!((next - previous).abs() > 1e-12);
        assert!((h.implied_quote().unwrap() / 0.002 - 1.0).abs() < 1e-12);
        previous = next;
    }
    Flag::lower(&flag);
    overnight.add_fixing(today - 1, 0.018).unwrap();
    assert!(Flag::is_up(&flag));
    Flag::lower(&flag);
    ibor.add_fixing(today, 0.08).unwrap();
    assert!(Flag::is_up(&flag));
    let next = fitted.discount_date(h.pillar_date(), false).unwrap();
    assert!((next - previous).abs() > 1e-4);
    assert!((h.implied_quote().unwrap() / 0.002 - 1.0).abs() < 1e-12);
    let previous_spot = h.earliest_date();
    Flag::lower(&flag);
    settings.set_evaluation_date(today + 1);
    assert!(Flag::is_up(&flag));
    assert!(h.earliest_date() > previous_spot);
    fitted.discount_date(h.pillar_date(), false).unwrap();
    assert!((h.implied_quote().unwrap() / 0.002 - 1.0).abs() < 1e-12);
    let weak = Shared::downgrade(&fitted);
    drop(fitted);
    assert!(weak.upgrade().is_none());
    assert!(h.implied_quote().is_err());
    let replacement_rate = shared(SimpleQuote::new(0.04));
    let replacement = curve(&replacement_rate).current_link().unwrap();
    Flag::lower(&flag);
    h.set_term_structure(&replacement);
    assert!(!Flag::is_up(&flag));
    let before = h.implied_quote().unwrap();
    replacement_rate.set_value(0.05);
    assert!(!Flag::is_up(&flag));
    assert!((h.implied_quote().unwrap() - before).abs() > 1e-3);
}
#[test]
fn malformed_inputs_and_date_updates_return_errors_and_recover() {
    let settings = shared(Settings::<Date>::new());
    let today = Date::new(23, Month::October, 2025);
    settings.set_evaluation_date(today);
    let curve = shared(FlatForward::with_rate(
        today,
        0.02,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>;
    let overnight = shared(Estr::new(Handle::new(curve.clone()), settings.clone()));
    let ibor = shared(Euribor::three_months(Handle::empty(), settings.clone()));
    let basis = Handle::new(shared(SimpleQuote::new(0.002)) as Shared<dyn Quote>);
    let build = |tenor, days, basis, ibor: &Shared<IborIndex>| {
        OvernightIborBasisSwapRateHelper::new(
            basis,
            tenor,
            days,
            Target::new(),
            BusinessDayConvention::ModifiedFollowing,
            false,
            &overnight,
            ibor,
            Handle::empty(),
        )
    };
    for tenor in [
        Period::new(0, TimeUnit::Years),
        Period::new(-1, TimeUnit::Years),
        Period::new(i32::MAX, TimeUnit::Years),
        Period::new(1, TimeUnit::Hours),
    ] {
        assert!(build(tenor, 2, basis.clone(), &ibor).is_err());
    }
    let tenor = Period::new(5, TimeUnit::Years);
    assert!(build(tenor, u32::MAX, basis.clone(), &ibor).is_err());
    assert!(build(tenor, 2, Handle::empty(), &ibor).is_err());
    let unrelated = shared(Euribor::three_months(
        Handle::empty(),
        shared(Settings::<Date>::new()),
    ));
    assert!(build(tenor, 2, basis.clone(), &unrelated).is_err());
    settings.reset_evaluation_date();
    assert!(build(tenor, 2, basis.clone(), &ibor).is_err());
    settings.set_evaluation_date(today);
    let h = build(tenor, 2, basis.clone(), &ibor).unwrap();
    h.set_term_structure(&curve);
    let initial_quote = h.implied_quote().unwrap();
    let initial_dates = (h.earliest_date(), h.maturity_date(), h.pillar_date());
    settings.set_evaluation_date(Date::max_date());
    assert!(build(tenor, 2, basis, &ibor).is_err());
    assert!(h.implied_quote().is_err());
    assert_eq!(
        (h.earliest_date(), h.maturity_date(), h.pillar_date()),
        initial_dates
    );
    settings.set_evaluation_date(today);
    assert_eq!(
        (h.earliest_date(), h.maturity_date(), h.pillar_date()),
        initial_dates
    );
    assert!((h.implied_quote().unwrap() - initial_quote).abs() < 1e-14);
}
