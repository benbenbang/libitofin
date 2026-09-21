use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::ibor::{Estr, Euribor, UsdLibor};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::loglinear::LogLinear;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::bootstraphelper::RateHelper;
use libitofin::termstructures::bootstraptraits::Discount;
use libitofin::termstructures::yields::{
    FlatForward, FraRateHelper, OISRateHelper, PiecewiseYieldCurve, Pillar, SwapRateHelper,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention::{
    ModifiedFollowing, Unadjusted,
};
use libitofin::time::calendars::{
    target::Target,
    unitedstates::{Market, UnitedStates},
};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::{
    actual360::Actual360,
    actual365fixed::Actual365Fixed,
    thirty360::{Convention, Thirty360},
};
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit::{Days, Months, Years};

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        actual.is_finite() && (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn quantlib_swap_last_relevant_date_and_discount() {
    let settings = shared(Settings::new());
    let today = Date::new(22, Month::December, 2016);
    settings.set_evaluation_date(today);
    let index = UsdLibor::new(Period::new(3, Months), Handle::empty(), settings).unwrap();
    let helper = SwapRateHelper::from_rate(
        0.02,
        Period::new(50, Years),
        UnitedStates::new(Market::GovernmentBond),
        Frequency::Semiannual,
        ModifiedFollowing,
        Thirty360::with_convention(Convention::BondBasis),
        &index,
    );
    assert_eq!(helper.maturity_date(), Date::new(28, Month::December, 2066));
    assert_eq!(
        helper.latest_relevant_date(),
        Date::new(29, Month::December, 2066)
    );
    let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        today,
        vec![helper.clone() as Shared<dyn RateHelper>],
        Actual365Fixed::new(),
        LogLinear,
    )
    .unwrap();
    near(
        curve.discount(1.0, false).unwrap(),
        0.9803084948945628,
        1e-12,
    );
    near(helper.implied_quote().unwrap(), 0.02, 1e-12);
}

#[test]
fn custom_pillars_are_transactional_and_recover_after_date_change() {
    let settings = shared(Settings::new());
    let today = Date::new(15, Month::March, 2024);
    settings.set_evaluation_date(today);
    let index = Euribor::six_months(Handle::empty(), settings.clone());
    let overnight = Estr::new(Handle::empty(), settings.clone());
    let quote = Handle::new(shared(SimpleQuote::new(0.03)) as Shared<dyn Quote>);
    let custom = Date::new(1, Month::July, 2024);
    let fra = FraRateHelper::try_new(
        quote.clone(),
        Period::new(1, Months),
        &index,
        true,
        Pillar::CustomDate(custom),
    )
    .unwrap();
    let swap = SwapRateHelper::try_with_details(
        quote.clone(),
        Period::new(2, Years),
        Target::new(),
        Frequency::Annual,
        Unadjusted,
        Actual360::new(),
        &index,
        Handle::empty(),
        Period::new(0, Days),
        None,
        Pillar::CustomDate(custom),
    )
    .unwrap();
    let ois = OISRateHelper::try_new(
        2,
        Period::new(2, Years),
        quote.clone(),
        &overnight,
        None,
        0,
        ModifiedFollowing,
        Frequency::Annual,
        Period::new(0, Days),
        Handle::empty(),
        Pillar::CustomDate(custom),
        RateAveraging::Compound,
        settings.clone(),
    )
    .unwrap();
    let helpers: Vec<Shared<dyn RateHelper>> = vec![fra.clone(), swap.clone(), ois.clone()];
    let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::with_rate(
        today,
        0.03,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let weak = Shared::downgrade(&curve);
    let dates = |h: &Shared<dyn RateHelper>| {
        (
            h.earliest_date(),
            h.maturity_date(),
            h.latest_relevant_date(),
            h.pillar_date(),
            h.latest_date(),
        )
    };
    let old_dates: Vec<_> = helpers.iter().map(dates).collect();
    let old_quotes: Vec<_> = helpers
        .iter()
        .map(|h| {
            h.set_term_structure(&curve);
            let value = h.implied_quote().unwrap();
            assert!(value.is_finite());
            value
        })
        .collect();
    for h in &helpers {
        assert_eq!(h.pillar_date(), custom);
    }
    settings.set_evaluation_date(Date::new(2, Month::July, 2024));
    assert!(fra.validate_dates().is_err());
    assert!(swap.validate_dates().is_err());
    assert!(ois.validate_dates().is_err());
    for (h, old) in helpers.iter().zip(old_dates) {
        assert_eq!(dates(h), old);
        assert!(
            h.implied_quote()
                .unwrap_err()
                .to_string()
                .contains("pillar date")
        );
    }
    settings.set_evaluation_date(today);
    fra.validate_dates().unwrap();
    swap.validate_dates().unwrap();
    ois.validate_dates().unwrap();
    for (h, value) in helpers.iter().zip(old_quotes) {
        near(h.implied_quote().unwrap(), value, 1e-12);
    }
    settings.set_evaluation_date(Date::max_date());
    for h in &helpers {
        assert!(h.implied_quote().is_err());
    }
    settings.set_evaluation_date(today);
    for h in &helpers {
        assert!(h.implied_quote().unwrap().is_finite());
    }
    assert!(
        FraRateHelper::try_from_months(
            quote.clone(),
            u32::MAX,
            &index,
            true,
            Pillar::LastRelevantDate
        )
        .is_err()
    );
    drop(curve);
    assert!(weak.upgrade().is_none());
    for h in &helpers {
        assert!(h.implied_quote().is_err());
    }
    for invalid in [Date::null(), today, Date::new(1, Month::January, 2030)] {
        assert!(
            FraRateHelper::try_new(
                quote.clone(),
                Period::new(1, Months),
                &index,
                true,
                Pillar::CustomDate(invalid)
            )
            .is_err()
        );
        assert!(
            SwapRateHelper::try_with_details(
                quote.clone(),
                Period::new(2, Years),
                Target::new(),
                Frequency::Annual,
                Unadjusted,
                Actual360::new(),
                &index,
                Handle::empty(),
                Period::new(0, Days),
                None,
                Pillar::CustomDate(invalid)
            )
            .is_err()
        );
        assert!(
            OISRateHelper::try_new(
                2,
                Period::new(2, Years),
                quote.clone(),
                &overnight,
                None,
                0,
                ModifiedFollowing,
                Frequency::Annual,
                Period::new(0, Days),
                Handle::empty(),
                Pillar::CustomDate(invalid),
                RateAveraging::Compound,
                settings.clone()
            )
            .is_err()
        );
    }
}
