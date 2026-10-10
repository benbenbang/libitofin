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
    Market { bond, today }
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
