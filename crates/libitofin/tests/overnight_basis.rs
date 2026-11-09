use libitofin::cashflows::{IborLeg, OvernightLeg, RateAveraging};
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::ibor::{estr::Estr, euribor::Euribor};
use libitofin::indexes::ibor::{sofr::Sofr, usdlibor::UsdLibor};
use libitofin::indexes::iborindex::{IborIndex, OvernightIndex};
use libitofin::indexes::interestrateindex::InterestRateIndex;
use libitofin::instrument::Instrument;
use libitofin::instruments::Swap;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::interpolations::loglinear::LogLinear;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::swap::DiscountingSwapEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::bootstraphelper::RateHelper;
use libitofin::termstructures::bootstraptraits::Discount;
use libitofin::termstructures::bootstraptraits::ZeroYield;
use libitofin::termstructures::globalbootstrap::GlobalBootstrap;
use libitofin::termstructures::multicurve::MultiCurve;
use libitofin::termstructures::yields::{
    FlatForward, OISRateHelper, OvernightIborBasisSwapRateHelper, PiecewiseYieldCurve, Pillar,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention::{Following, ModifiedFollowing};
use libitofin::time::calendars::target::Target;
use libitofin::time::calendars::unitedstates::{Market as UsMarket, UnitedStates};
use libitofin::time::date::{Date, Month};
use libitofin::time::dategenerationrule::DateGeneration;
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::schedule::MakeSchedule;
use libitofin::time::timeunit::TimeUnit::{Days, Months, Years};

fn quote(value: f64) -> Handle<dyn Quote> {
    Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
}

fn flat(today: Date, rate: f64) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(
        today,
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}

fn helper(
    years: i32,
    basis: Handle<dyn Quote>,
    overnight: &Shared<OvernightIndex>,
    ibor: &Shared<IborIndex>,
    discount: Handle<dyn YieldTermStructure>,
) -> Shared<OvernightIborBasisSwapRateHelper> {
    OvernightIborBasisSwapRateHelper::new(
        basis,
        Period::new(years, Years),
        2,
        Target::new(),
        ModifiedFollowing,
        false,
        overnight,
        ibor,
        discount,
    )
    .unwrap()
}

fn swap_npv(
    h: &OvernightIborBasisSwapRateHelper,
    years: i32,
    basis: f64,
    overnight: &Shared<OvernightIndex>,
    ibor: &Shared<IborIndex>,
    discount: Handle<dyn YieldTermStructure>,
    settings: &Shared<Settings<Date>>,
) -> f64 {
    let schedule = MakeSchedule::new()
        .from(h.earliest_date())
        .to(h.earliest_date() + Period::new(years, Years))
        .with_tenor(ibor.tenor())
        .with_calendar(Target::new())
        .with_convention(ModifiedFollowing)
        .backwards()
        .build();
    let overnight_leg = OvernightLeg::new(schedule.clone(), overnight.clone())
        .with_notional(1.0)
        .with_spread(basis)
        .build()
        .unwrap();
    let ibor_leg = IborLeg::new(schedule, ibor.clone())
        .with_notional(1.0)
        .build()
        .unwrap();
    let mut swap = Swap::two_leg(overnight_leg, ibor_leg, settings.clone());
    swap.base_mut()
        .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
            discount,
            None,
            None,
            None,
            settings.clone(),
        )) as SharedMut<dyn PricingEngine>);
    swap.npv().unwrap()
}

#[test]
fn overnight_basis_flat_curves_zero_independent_swaps() {
    let settings = shared(Settings::<Date>::new());
    let today = Date::new(23, Month::October, 2025);
    settings.set_evaluation_date(today);
    let overnight = shared(Estr::new(flat(today, 0.021), settings.clone()));
    let ibor_curve = flat(today, 0.034);
    let ibor = shared(Euribor::three_months(ibor_curve.clone(), settings.clone()));
    for explicit in [false, true] {
        let discount = if explicit {
            flat(today, 0.012)
        } else {
            Handle::empty()
        };
        let h = helper(5, quote(0.002), &overnight, &ibor, discount.clone());
        assert!(h.implied_quote().is_err());
        h.set_term_structure(&ibor_curve.current_link().unwrap());
        let basis = h.implied_quote().unwrap();
        let pricing = if explicit {
            discount
        } else {
            ibor_curve.clone()
        };
        assert!(swap_npv(&h, 5, basis, &overnight, &ibor, pricing, &settings).abs() < 1e-10);
        assert_eq!(h.earliest_date(), Date::new(27, Month::October, 2025));
        assert_eq!(h.maturity_date(), Date::new(28, Month::October, 2030));
        assert_eq!(h.pillar_date(), h.latest_relevant_date());
    }
}

#[test]
fn overnight_basis_coupled_curves_reprice_and_recalculate() {
    let settings = shared(Settings::<Date>::new());
    let today = Date::new(23, Month::October, 2025);
    settings.set_evaluation_date(today);
    let overnight_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
    let ibor_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
    let overnight = shared(Estr::new(overnight_handle.handle(), settings.clone()));
    let ibor = shared(Euribor::three_months(
        ibor_handle.handle(),
        settings.clone(),
    ));
    let market_basis = shared(SimpleQuote::new(0.002));
    let basis = Handle::new(market_basis.clone() as Shared<dyn Quote>);
    let mut overnight_helpers: Vec<Shared<dyn RateHelper>> = Vec::new();
    let mut basis_helpers = Vec::new();
    for year in 1..=5 {
        overnight_helpers.push(OISRateHelper::new(
            2,
            Period::new(year, Years),
            quote(0.02 + f64::from(year) * 0.001),
            &overnight,
            Some(ibor_handle.handle()),
            0,
            Following,
            Frequency::Annual,
            Period::new(0, Days),
            Handle::empty(),
            Pillar::LastRelevantDate,
            RateAveraging::Compound,
            settings.clone(),
        ));
        basis_helpers.push(helper(
            year,
            basis.clone(),
            &overnight,
            &ibor,
            Handle::empty(),
        ));
    }
    let build = |helpers| {
        PiecewiseYieldCurve::<Discount, LogLinear, GlobalBootstrap>::with_bootstrap(
            today,
            helpers,
            Actual360::new(),
            LogLinear,
            GlobalBootstrap::new(Some(1e-12), None, Vec::new()),
        )
        .unwrap()
    };
    let overnight_curve = build(overnight_helpers.clone());
    let ibor_curve = build(
        basis_helpers
            .iter()
            .map(|h| h.clone() as Shared<dyn RateHelper>)
            .collect(),
    );
    let multicurve = MultiCurve::new(1e-12);
    let _overnight_owner = multicurve
        .add_bootstrapped_curve(&overnight_handle, overnight_curve)
        .unwrap();
    let ibor_owner = multicurve
        .add_bootstrapped_curve(&ibor_handle, ibor_curve)
        .unwrap();
    for basis_value in [0.002, 0.003] {
        market_basis.set_value(basis_value);
        ibor_owner
            .current_link()
            .unwrap()
            .discount_date(basis_helpers.last().unwrap().pillar_date(), false)
            .unwrap();
        for (i, h) in basis_helpers.iter().enumerate() {
            check_fixture(2, basis_value, i + 1, h, &ibor_owner);
            assert!((h.implied_quote().unwrap() / basis_value - 1.0).abs() <= 1e-12);
            assert!(
                swap_npv(
                    h,
                    i as i32 + 1,
                    basis_value,
                    &overnight,
                    &ibor,
                    ibor_owner.clone(),
                    &settings
                )
                .abs()
                    < 1e-10
            );
        }
        for h in &overnight_helpers {
            assert!(
                (h.implied_quote().unwrap() / h.quote().current_link().unwrap().value().unwrap()
                    - 1.0)
                    .abs()
                    <= 1e-12
            );
        }
    }
}

fn check_fixture(
    mode: usize,
    basis: f64,
    year: usize,
    h: &OvernightIborBasisSwapRateHelper,
    curve: &Handle<dyn YieldTermStructure>,
) {
    let fields = include_str!("fixtures/overnight_basis/quantlib.csv")
        .lines()
        .map(|line| {
            line.split(',')
                .map(|v| v.parse::<f64>().unwrap())
                .collect::<Vec<_>>()
        })
        .find(|v| v[0] == mode as f64 && v[1] == basis && v[2] == year as f64)
        .unwrap();
    assert_eq!(h.earliest_date().serial_number(), fields[3] as i32);
    assert_eq!(h.maturity_date().serial_number(), fields[4] as i32);
    assert_eq!(h.pillar_date().serial_number(), fields[5] as i32);
    let actual = curve
        .current_link()
        .unwrap()
        .discount_date(h.pillar_date(), false)
        .unwrap();
    assert!(
        (actual / fields[6] - 1.0).abs() < 1e-12,
        "mode={mode}, basis={basis}, year={year}: {actual} vs {}",
        fields[6]
    );
}

#[test]
fn overnight_basis_explicit_and_default_discount_match_quantlib() {
    let settings = shared(Settings::<Date>::new());
    let today = Date::new(23, Month::October, 2025);
    settings.set_evaluation_date(today);
    let overnight = shared(Estr::new(flat(today, 0.021), settings.clone()));
    for explicit in [false, true] {
        let discount = if explicit {
            flat(today, 0.012)
        } else {
            Handle::empty()
        };
        let forecast = RelinkableHandle::<dyn YieldTermStructure>::empty();
        let ibor = shared(Euribor::three_months(forecast.handle(), settings.clone()));
        let helpers: Vec<_> = (1..=5)
            .map(|year| helper(year, quote(0.002), &overnight, &ibor, discount.clone()))
            .collect();
        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            today,
            helpers
                .iter()
                .map(|h| h.clone() as Shared<dyn RateHelper>)
                .collect(),
            Actual360::new(),
            LogLinear,
        )
        .unwrap();
        forecast.link_to(curve as Shared<dyn YieldTermStructure>);
        for (i, h) in helpers.iter().enumerate() {
            check_fixture(usize::from(explicit), 0.002, i + 1, h, &forecast.handle());
            assert!((h.implied_quote().unwrap() / 0.002 - 1.0).abs() < 1e-12);
            let pricing = if explicit {
                discount.clone()
            } else {
                forecast.handle()
            };
            assert!(
                swap_npv(
                    h,
                    i as i32 + 1,
                    0.002,
                    &overnight,
                    &ibor,
                    pricing,
                    &settings
                )
                .abs()
                    < 1e-10
            );
        }
    }
}

/// `testOvernightIborBootstrap` (v1.44 `basisswapratehelpers.cpp:147-246`) on
/// the default schedule rule, fitting the Libor curve with an empty discount
/// handle (`testOvernightIborBootstrapWithoutDiscountCurve` `:560`) or an
/// external one (`testOvernightIborBootstrapWithDiscountCurve` `:566`): every
/// quoted swap rebuilt backward from the unadjusted `spot + tenor` is fair
/// within `1e-8` on notional 100. The C++ fixture takes today from the clock;
/// this pins 23 October 2025, where the 5Y termination is a Sunday. The
/// `bootstrapBaseCurve` cases `:579` and `:585` are deferred (#1423).
#[test]
fn overnight_ibor_bootstrap_reprices_fair_swaps_on_backward_schedules() {
    let quotes = [
        (1, 0.0010),
        (2, 0.0012),
        (3, 0.0015),
        (5, 0.0015),
        (8, 0.0018),
        (10, 0.0020),
        (15, 0.0021),
        (20, 0.0021),
    ];
    let calendar = UnitedStates::new(UsMarket::GovernmentBond);
    for external in [false, true] {
        let settings = shared(Settings::<Date>::new());
        let today = Date::new(23, Month::October, 2025);
        settings.set_evaluation_date(today);
        let flat365 = |rate: f64| {
            Handle::new(shared(FlatForward::with_rate(
                today,
                rate,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        };
        let discount = if external {
            flat365(0.005)
        } else {
            Handle::empty()
        };
        let sofr = shared(Sofr::new(flat365(0.01), settings.clone()));
        let libor = |curve| shared(UsdLibor::six_months(curve, settings.clone()));
        let helpers = quotes
            .iter()
            .map(|&(years, basis)| {
                OvernightIborBasisSwapRateHelper::new(
                    quote(basis),
                    Period::new(years, Years),
                    2,
                    calendar.clone(),
                    Following,
                    false,
                    &sofr,
                    &libor(Handle::empty()),
                    discount.clone(),
                )
                .unwrap() as Shared<dyn RateHelper>
            })
            .collect();
        let fitted = Handle::new(
            PiecewiseYieldCurve::<ZeroYield, Linear>::new(
                today,
                helpers,
                Actual365Fixed::new(),
                Linear,
            )
            .unwrap() as Shared<dyn YieldTermStructure>,
        );
        let libor = libor(fitted.clone());
        let pricing = if external { discount } else { fitted };
        let spot = calendar.advance(today, 2, Days, Following, false);
        for (years, basis) in quotes {
            let schedule = MakeSchedule::new()
                .from(spot)
                .to(spot + Period::new(years, Years))
                .with_tenor(libor.tenor())
                .with_calendar(calendar.clone())
                .with_convention(Following)
                .backwards()
                .build();
            let overnight_leg = OvernightLeg::new(schedule.clone(), sofr.clone())
                .with_notional(100.0)
                .with_spread(basis)
                .build()
                .unwrap();
            let ibor_leg = IborLeg::new(schedule, libor.clone())
                .with_notional(100.0)
                .build()
                .unwrap();
            let mut swap = Swap::two_leg(overnight_leg, ibor_leg, settings.clone());
            swap.base_mut()
                .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                    pricing.clone(),
                    None,
                    None,
                    None,
                    settings.clone(),
                )) as SharedMut<dyn PricingEngine>);
            let npv = swap.npv().unwrap();
            assert!(npv.abs() < 1e-8, "external={external}, {years}Y NPV {npv}");
        }
    }
}

/// The default rule is `Backward` (v1.44 `basisswapratehelpers.hpp:147`): on a
/// 15M tenor paced by 6M Euribor the short stub sits at the front, so the
/// implied basis zeroes the backward swap but not the forward one (the v1.44
/// `:531-557` stub-placement check, on the default rule).
#[test]
fn overnight_basis_default_rule_places_the_stub_at_the_front() {
    let settings = shared(Settings::<Date>::new());
    let today = Date::new(23, Month::October, 2025);
    settings.set_evaluation_date(today);
    let overnight = shared(Estr::new(flat(today, 0.021), settings.clone()));
    let ibor_curve = flat(today, 0.034);
    let ibor = shared(Euribor::six_months(ibor_curve.clone(), settings.clone()));
    let discount = flat(today, 0.012);
    let tenor = Period::new(15, Months);
    let h = OvernightIborBasisSwapRateHelper::new(
        quote(0.002),
        tenor,
        2,
        Target::new(),
        ModifiedFollowing,
        false,
        &overnight,
        &ibor,
        discount.clone(),
    )
    .unwrap();
    h.set_term_structure(&ibor_curve.current_link().unwrap());
    let basis = h.implied_quote().unwrap();
    let npv = |rule| {
        let schedule = MakeSchedule::new()
            .from(h.earliest_date())
            .to(h.earliest_date() + tenor)
            .with_tenor(ibor.tenor())
            .with_calendar(Target::new())
            .with_convention(ModifiedFollowing)
            .with_rule(rule)
            .build();
        let overnight_leg = OvernightLeg::new(schedule.clone(), overnight.clone())
            .with_notional(1.0)
            .with_spread(basis)
            .build()
            .unwrap();
        let ibor_leg = IborLeg::new(schedule, ibor.clone())
            .with_notional(1.0)
            .build()
            .unwrap();
        let mut swap = Swap::two_leg(overnight_leg, ibor_leg, settings.clone());
        swap.base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                discount.clone(),
                None,
                None,
                None,
                settings.clone(),
            )) as SharedMut<dyn PricingEngine>);
        swap.npv().unwrap()
    };
    let backward = npv(DateGeneration::Backward);
    let forward = npv(DateGeneration::Forward);
    assert!(backward.abs() < 1e-12, "backward NPV {backward}");
    assert!(forward.abs() > 1e-9, "forward NPV {forward}");
}
