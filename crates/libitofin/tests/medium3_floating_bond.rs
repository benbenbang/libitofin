use libitofin::cashflows::{BlackIborCouponPricer, FloatingRateCouponPricer};
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::ibor::Euribor;
use libitofin::indexes::iborindex::IborIndex;
use libitofin::indexes::index::Index;
use libitofin::instrument::Instrument;
use libitofin::instruments::FloatingRateBond;
use libitofin::interestrate::Compounding;
use libitofin::pricingengines::bond::DiscountingBondEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::{
    ConstantOptionletVolatility, OptionletVolatilityStructure, VolatilityType,
};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::dategenerationrule::DateGeneration;
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::schedule::Schedule;
use libitofin::time::timeunit::TimeUnit;
use libitofin::utilities::null::Null;

struct Case {
    name: &'static str,
    at_par: bool,
    gearings: Vec<f64>,
    spreads: Vec<f64>,
    caps: Vec<f64>,
    floors: Vec<f64>,
    known: bool,
    lag: Option<u32>,
    following: bool,
}

fn case(name: &'static str) -> Case {
    let mut c = Case {
        name,
        at_par: true,
        gearings: vec![],
        spreads: vec![],
        caps: vec![],
        floors: vec![],
        known: false,
        lag: None,
        following: false,
    };
    match name {
        "plain_indexed" => c.at_par = false,
        "collar" | "negative" | "mixed" | "sentinel" => {
            c.gearings = vec![1.2];
            c.spreads = vec![0.004];
            c.caps = vec![0.04];
            c.floors = vec![0.025];
            if name == "sentinel" {
                c.gearings = vec![0.0, 1.2, 0.0];
                c.spreads = vec![0.1, 0.004, 0.001];
                c.caps = vec![f64::null(), 0.04, f64::null()];
                c.floors = vec![f64::null(), 0.025, f64::null()];
            }
            if name == "negative" {
                c.gearings = vec![-1.5];
                c.spreads = vec![0.08];
                c.floors = vec![0.015];
            }
            if name == "mixed" {
                c.gearings = vec![0.0, 1.2, 0.0];
                c.spreads = vec![0.1, 0.004, 0.001];
                c.caps = vec![0.045];
                c.floors = vec![0.02];
            }
        }
        "fixed" => {
            c.gearings = vec![0.0];
            c.spreads = vec![0.01];
            c.caps = vec![0.008];
            c.floors = vec![0.02];
        }
        "ex_known" => {
            c.known = true;
            c.gearings = vec![1.0];
            c.spreads = vec![0.004];
            c.caps = vec![0.04];
            c.floors = vec![0.02];
        }
        "holiday" => {
            c.lag = Some(0);
            c.following = true;
        }
        "plain_par" | "long" => {}
        _ => panic!("unknown case"),
    }
    c
}

fn schedule() -> Schedule {
    Schedule::with_metadata(
        vec![
            Date::new(1, Month::February, 2026),
            Date::new(15, Month::May, 2026),
            Date::new(1, Month::November, 2026),
            Date::new(28, Month::February, 2027),
        ],
        Target::new(),
        Bdc::Unadjusted,
        Some(Bdc::Unadjusted),
        Some(Period::new(6, TimeUnit::Months)),
        Some(DateGeneration::Backward),
        Some(false),
        vec![false, true, false],
    )
}

fn long_schedule() -> Schedule {
    Schedule::with_metadata(
        vec![
            Date::new(1, Month::February, 2026),
            Date::new(15, Month::October, 2026),
            Date::new(15, Month::April, 2027),
            Date::new(30, Month::January, 2028),
        ],
        Target::new(),
        Bdc::Unadjusted,
        Some(Bdc::Unadjusted),
        Some(Period::new(6, TimeUnit::Months)),
        Some(DateGeneration::Backward),
        Some(false),
        vec![false, true, false],
    )
}

fn curve(today: Date, rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        today,
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

fn construct(
    c: &Case,
    s: Schedule,
    index: Shared<IborIndex>,
    settings: Shared<Settings<Date>>,
    issue: Option<Date>,
) -> libitofin::errors::QlResult<FloatingRateBond> {
    FloatingRateBond::new(
        2,
        250.0,
        s,
        index,
        Actual360::new(),
        Bdc::Following,
        c.lag,
        c.gearings.clone(),
        c.spreads.clone(),
        c.caps.clone(),
        c.floors.clone(),
        97.0,
        issue,
        c.known.then_some(Period::new(5, TimeUnit::Days)),
        Target::new(),
        Bdc::Preceding,
        false,
        if c.following {
            Bdc::Following
        } else {
            Bdc::Preceding
        },
        settings,
    )
}

struct Market {
    bond: FloatingRateBond,
    settings: Shared<Settings<Date>>,
    forecast: RelinkableHandle<dyn YieldTermStructure>,
    discount: RelinkableHandle<dyn YieldTermStructure>,
    vol_quote: Shared<SimpleQuote>,
    today: Date,
}

fn market(c: &Case) -> Market {
    let today = if c.known {
        Date::new(12, Month::May, 2026)
    } else {
        Date::new(5, Month::January, 2026)
    };
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    settings.set_using_at_par_coupons(c.at_par);
    let forecast = RelinkableHandle::new(curve(today, 0.03));
    let discount = RelinkableHandle::new(curve(today, 0.025));
    let index = shared(Euribor::six_months(forecast.handle(), settings.clone()));
    if c.known {
        index
            .add_fixing(Date::new(29, Month::January, 2026), 0.035)
            .unwrap();
    }
    let mut bond = construct(
        c,
        if c.name == "long" {
            long_schedule()
        } else {
            schedule()
        },
        index,
        settings.clone(),
        Some(Date::new(1, Month::February, 2026)),
    )
    .unwrap();
    let vol_quote = shared(SimpleQuote::new(0.2));
    let vol = shared(ConstantOptionletVolatility::with_quote(
        today,
        Target::new(),
        Bdc::Following,
        Handle::new(vol_quote.clone() as Shared<dyn Quote>),
        Actual360::new(),
        VolatilityType::ShiftedLognormal,
        0.0,
    )) as Shared<dyn OptionletVolatilityStructure>;
    bond.set_coupon_pricer(
        shared_mut(BlackIborCouponPricer::with_vol(Handle::new(vol)))
            as SharedMut<dyn FloatingRateCouponPricer>,
    );
    bond.bond_mut()
        .base_mut()
        .set_pricing_engine(shared_mut(DiscountingBondEngine::new(
            discount.handle(),
            None,
            settings.clone(),
        )));
    Market {
        bond,
        settings,
        forecast,
        discount,
        vol_quote,
        today,
    }
}

fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}"
    );
}

#[test]
fn native_quantlib_coupon_geometry_rates_and_discounted_prices() {
    for name in [
        "plain_par",
        "long",
        "plain_indexed",
        "collar",
        "sentinel",
        "negative",
        "mixed",
        "fixed",
        "ex_known",
        "holiday",
    ] {
        let c = case(name);
        let mut m = market(&c);
        for line in include_str!("fixtures/medium3_floating_bond/oracle.tsv")
            .lines()
            .filter(|line| line.starts_with(c.name))
        {
            let fields: Vec<_> = line.split('\t').collect();
            let n = |i: usize| fields[i].parse::<f64>().unwrap();
            if fields[1] == "coupon" {
                let i = n(2) as usize;
                let flow = &m.bond.bond().cashflows()[i];
                let coupon = flow.as_coupon().unwrap();
                for (date, j) in [
                    (flow.date(), 3),
                    (coupon.accrual_start_date(), 4),
                    (coupon.accrual_end_date(), 5),
                    (coupon.reference_period_start(), 6),
                    (coupon.reference_period_end(), 7),
                ] {
                    assert_eq!(date.serial_number(), n(j) as i32, "{name} coupon {i}");
                }
                assert_eq!(
                    flow.ex_coupon_date().map_or(0, |d| d.serial_number()),
                    n(8) as i32
                );
                match &m.bond.ibor_coupons()[i] {
                    Some(ibor) => {
                        assert_eq!(ibor.fixing_date().serial_number(), n(9) as i32);
                        assert_eq!(c.lag.unwrap_or(2), n(10) as u32);
                        near(ibor.gearing(), n(11), 0.0);
                        near(ibor.spread(), n(12), 0.0);
                    }
                    None => assert_eq!(n(9), 0.0),
                }
                near(coupon.rate().unwrap(), n(13), 1e-13);
                near(flow.amount().unwrap(), n(14), 1e-11);
                near(coupon.accrual_period(), n(15), 1e-15);
            } else {
                assert_eq!(m.today.serial_number(), n(2) as i32);
                assert_eq!(
                    m.bond.bond().settlement_date(None).unwrap().serial_number(),
                    n(3) as i32
                );
                assert_eq!(
                    m.bond.bond().maturity_date().unwrap().serial_number(),
                    n(4) as i32
                );
                near(m.bond.bond_mut().npv().unwrap(), n(5), 1e-10);
                near(m.bond.bond_mut().dirty_price().unwrap(), n(6), 1e-10);
                near(m.bond.bond_mut().clean_price().unwrap(), n(7), 1e-10);
                near(m.bond.bond().accrued_amount(None).unwrap(), n(8), 1e-12);
                near(m.bond.bond().notional(None).unwrap(), n(9), 0.0);
                near(m.bond.bond().redemptions()[0].amount().unwrap(), n(10), 0.0);
            }
        }
        assert_eq!(m.bond.bond().cashflows().len(), 4);
        assert_eq!(m.bond.bond().redemptions().len(), 1);
        assert_eq!(m.bond.bond().notionals(), &[250.0, 0.0]);
        assert_ne!(
            m.bond.bond().maturity_date().unwrap(),
            m.bond.bond().redemptions()[0].date()
        );
    }
}

#[test]
fn index_curve_volatility_pricer_and_settings_invalidate_lazy_prices() {
    let mut m = market(&case("collar"));
    let initial = m.bond.bond_mut().npv().unwrap();
    assert!(m.bond.bond().base().is_calculated());
    m.forecast.link_to(curve(m.today, 0.035));
    assert!(!m.bond.bond().base().is_calculated());
    let changed_forward = m.bond.bond_mut().npv().unwrap();
    assert!(changed_forward > initial);
    m.vol_quote.set_value(0.35);
    assert!(!m.bond.bond().base().is_calculated());
    let changed_vol = m.bond.bond_mut().npv().unwrap();
    assert!((changed_vol - changed_forward).abs() > 1e-5);
    m.discount.link_to(curve(m.today, 0.04));
    assert!(!m.bond.bond().base().is_calculated());
    assert!(m.bond.bond_mut().npv().unwrap() < changed_vol);
    m.bond
        .set_coupon_pricer(shared_mut(BlackIborCouponPricer::new()));
    assert!(!m.bond.bond().base().is_calculated());
    assert!(m.bond.bond_mut().npv().is_err());
    m.settings
        .set_evaluation_date(Date::new(2, Month::March, 2027));
    assert!(!m.bond.bond().base().is_calculated());
    assert!(m.bond.bond().is_expired().unwrap());
    assert_eq!(m.bond.bond_mut().npv().unwrap(), 0.0);
    assert_eq!(m.bond.bond_mut().dirty_price().unwrap(), 0.0);
}

#[test]
fn fixed_only_observes_index_and_survives_into_base_without_forecasts() {
    let mut m = market(&case("fixed"));
    let initial = m.bond.bond_mut().npv().unwrap();
    assert!(m.bond.ibor_coupons().iter().all(Option::is_none));
    m.forecast.link_to(curve(m.today, 0.06));
    assert!(!m.bond.bond().base().is_calculated());
    near(m.bond.bond_mut().npv().unwrap(), initial, 0.0);
    m.bond
        .index()
        .add_fixing(Date::new(29, Month::January, 2026), 0.1)
        .unwrap();
    assert!(!m.bond.bond().base().is_calculated());
    let mut base = m.bond.into_bond();
    near(base.npv().unwrap(), initial, 0.0);
    m.forecast.link_to(curve(m.today, 0.07));
    assert!(!base.base().is_calculated());
    near(base.npv().unwrap(), initial, 0.0);
}

#[test]
fn known_fixing_missing_then_added_and_cleared_is_observed() {
    let m = market(&case("ex_known"));
    let mut bond = m.bond;
    let amount = bond.bond().cashflows()[0].amount().unwrap();
    bond.index().clear_fixings();
    assert!(bond.bond().cashflows()[0].amount().is_err());
    bond.index()
        .add_fixing(Date::new(29, Month::January, 2026), 0.025)
        .unwrap();
    assert!(bond.bond().cashflows()[0].amount().unwrap() < amount);
    let initial = bond.bond_mut().npv().unwrap();
    bond.index().clear_fixings();
    assert!(!bond.bond().base().is_calculated());
    near(bond.bond_mut().npv().unwrap(), initial, 0.0);
    assert!(bond.bond().accrued_amount(None).is_err());
}

#[test]
fn default_pricer_cap_pricer_requirement_and_late_issue_date_match_source() {
    let m = market(&case("plain_par"));
    let index = Shared::clone(m.bond.index());
    let plain = construct(
        &case("plain_par"),
        schedule(),
        index.clone(),
        m.settings.clone(),
        None,
    )
    .unwrap();
    assert!(plain.bond().cashflows()[0].amount().is_ok());
    let capped = construct(
        &case("collar"),
        schedule(),
        index.clone(),
        m.settings.clone(),
        None,
    )
    .unwrap();
    assert!(
        capped.bond().cashflows()[0]
            .amount()
            .unwrap_err()
            .message()
            .contains("pricer not set")
    );
    let mut absent = case("plain_par");
    absent.caps = vec![f64::null()];
    let sentinel = construct(&absent, schedule(), index.clone(), m.settings.clone(), None).unwrap();
    assert!(
        sentinel.bond().cashflows()[0]
            .amount()
            .unwrap_err()
            .message()
            .contains("pricer not set")
    );
    let late = Date::new(1, Month::December, 2026);
    let bond = construct(
        &case("plain_par"),
        schedule(),
        index,
        m.settings,
        Some(late),
    )
    .unwrap();
    assert_eq!(bond.bond().issue_date(), Some(late));
    assert_eq!(bond.bond().settlement_date(None).unwrap(), late);
}

#[test]
fn invalid_schedules_vectors_and_floating_collars_return_errors() {
    let m = market(&case("plain_par"));
    let index = Shared::clone(m.bond.index());
    for dates in [vec![], vec![m.today], vec![Date::null(), m.today]] {
        assert!(
            construct(
                &case("plain_par"),
                Schedule::from_dates(dates),
                index.clone(),
                m.settings.clone(),
                None
            )
            .is_err()
        );
    }
    for field in 0..4 {
        let mut c = case("plain_par");
        let vector = match field {
            0 => &mut c.gearings,
            1 => &mut c.spreads,
            2 => &mut c.caps,
            _ => &mut c.floors,
        };
        *vector = vec![0.01; 4];
        assert!(construct(&c, schedule(), index.clone(), m.settings.clone(), None).is_err());
    }
    let mut c = case("collar");
    c.caps = vec![0.01];
    assert!(construct(&c, schedule(), index.clone(), m.settings.clone(), None).is_err());
    c.gearings = vec![0.0];
    assert!(construct(&c, schedule(), index, m.settings, None).is_ok());
}

#[test]
fn nonfinite_coupon_inputs_and_mismatched_settings_are_rejected() {
    let m = market(&case("plain_par"));
    let index = Shared::clone(m.bond.index());
    assert!(
        construct(
            &case("plain_par"),
            schedule(),
            index.clone(),
            shared(Settings::new()),
            None
        )
        .is_err()
    );
    let mut fixed = case("fixed");
    fixed.caps.clear();
    fixed.floors = vec![f64::MAX];
    let huge = construct(&fixed, schedule(), index.clone(), m.settings.clone(), None).unwrap();
    assert_eq!(
        huge.bond().cashflows()[0]
            .as_coupon()
            .unwrap()
            .rate()
            .unwrap(),
        f64::MAX
    );
    let mut geared = case("plain_par");
    geared.gearings = vec![f64::null()];
    let huge_gearing =
        construct(&geared, schedule(), index.clone(), m.settings.clone(), None).unwrap();
    assert_eq!(
        huge_gearing.ibor_coupons()[0].as_ref().unwrap().gearing(),
        f64::null()
    );
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for field in 0..4 {
            let mut c = case("plain_par");
            let vector = match field {
                0 => &mut c.gearings,
                1 => &mut c.spreads,
                2 => &mut c.caps,
                _ => &mut c.floors,
            };
            *vector = vec![value];
            assert!(construct(&c, schedule(), index.clone(), m.settings.clone(), None).is_err());
        }
        for (face, redemption) in [(value, 100.0), (100.0, value), (f64::MAX / 2.0, 1000.0)] {
            assert!(
                FloatingRateBond::new(
                    2,
                    face,
                    schedule(),
                    index.clone(),
                    Actual360::new(),
                    Bdc::Following,
                    None,
                    vec![],
                    vec![],
                    vec![],
                    vec![],
                    redemption,
                    None,
                    None,
                    Target::new(),
                    Bdc::Unadjusted,
                    false,
                    Bdc::Preceding,
                    m.settings.clone(),
                )
                .is_err()
            );
        }
    }
}
