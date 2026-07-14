//! `test-suite/capflooredcoupon.cpp` `CommonVars`: `testLargeRates` (:188)
//! and `testDecomposition` (:232). A 20-year annual leg over Euribor 1Y on a
//! flat 5% continuous `Actual/Actual (ISDA)` curve, volatility 0.20. The
//! evaluation date is `Date::todaysDate()` in C++; the port pins a fixed
//! TARGET business day, the decomposition identities being self-consistent
//! for any date. Every capped-leg swap NPV is checked against its vanilla
//! leg less (or plus) the matching cap/floor/collar instrument.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::couponpricer::{BlackIborCouponPricer, FloatingRateCouponPricer};
use crate::cashflows::{FixedRateLeg, IborCoupon, IborLeg, set_coupon_pricer};
use crate::handle::Handle;
use crate::indexes::ibor::Euribor;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::{CapFloor, Swap};
use crate::interestrate::Compounding;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::{BlackCapFloorEngine, DiscountingSwapEngine};
use crate::quotes::make_quote_handle;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::{
    ConstantOptionletVolatility, OptionletVolatilityStructure, VolatilityType,
};
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::calendars::target::Target;
use crate::time::date::{Date, Month};
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::daycounters::actualactual::{ActualActual, Convention as ActualActualConvention};
use crate::time::daycounters::thirty360::{Convention as Thirty360Convention, Thirty360};
use crate::time::frequency::Frequency;
use crate::time::schedule::{MakeSchedule, Schedule};
use crate::time::timeunit::TimeUnit;
use crate::types::{Rate, Real, Spread, Volatility};

const MODIFIED_FOLLOWING: BusinessDayConvention = BusinessDayConvention::ModifiedFollowing;

struct Vars {
    settings: Shared<Settings<Date>>,
    calendar: Calendar,
    curve: Handle<dyn YieldTermStructure>,
    index: Shared<IborIndex>,
    start_date: Date,
    length: i32,
    nominal: Real,
    volatility: Volatility,
}

impl Vars {
    fn new() -> Vars {
        let calendar = Target::new();
        let today = calendar.adjust(
            Date::new(15, Month::June, 2026),
            BusinessDayConvention::Following,
        );
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let curve: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
            settlement,
            0.05,
            ActualActual::with_convention(ActualActualConvention::ISDA),
            Compounding::Continuous,
            Frequency::Annual,
        ))
            as Shared<dyn YieldTermStructure>);
        let index = shared(Euribor::one_year(curve.clone(), settings.clone()));
        Vars {
            settings,
            calendar,
            curve,
            index,
            start_date: settlement,
            length: 20,
            nominal: 100.0,
            volatility: 0.20,
        }
    }

    fn schedule(&self) -> Schedule {
        let end = self.calendar.advance(
            self.start_date,
            self.length,
            TimeUnit::Years,
            MODIFIED_FOLLOWING,
            false,
        );
        MakeSchedule::new()
            .from(self.start_date)
            .to(end)
            .with_frequency(Frequency::Annual)
            .with_calendar(self.calendar.clone())
            .with_convention(MODIFIED_FOLLOWING)
            .with_termination_date_convention(MODIFIED_FOLLOWING)
            .forwards()
            .build()
    }

    fn fixed_leg(&self) -> Leg {
        FixedRateLeg::new(self.schedule())
            .with_notional(self.nominal)
            .with_coupon_rate(
                0.0,
                Thirty360::with_convention(Thirty360Convention::BondBasis),
                Compounding::Simple,
                Frequency::Annual,
            )
            .unwrap()
            .build()
            .unwrap()
    }

    fn ibor_leg(&self, gearing: Real, spread: Spread) -> IborLeg {
        IborLeg::new(self.schedule(), self.index.clone())
            .with_notional(self.nominal)
            .with_payment_day_counter(self.index.day_counter().clone())
            .with_payment_adjustment(MODIFIED_FOLLOWING)
            .with_fixing_days(2)
            .with_gearing(gearing)
            .with_spread(spread)
    }

    fn float_coupons(&self, gearing: Real, spread: Spread) -> Vec<Shared<IborCoupon>> {
        self.ibor_leg(gearing, spread).coupons().unwrap()
    }

    fn capped_floored_leg(
        &self,
        caps: Vec<Rate>,
        floors: Vec<Rate>,
        gearing: Real,
        spread: Spread,
    ) -> Leg {
        let mut builder = self.ibor_leg(gearing, spread);
        if !caps.is_empty() {
            builder = builder.with_caps(caps);
        }
        if !floors.is_empty() {
            builder = builder.with_floors(floors);
        }
        let coupons = builder.capped_floored_coupons().unwrap();
        set_coupon_pricer(&coupons, self.vol_pricer());
        coupons
            .into_iter()
            .map(|coupon| coupon as Shared<dyn CashFlow>)
            .collect()
    }

    fn vol_pricer(&self) -> SharedMut<dyn FloatingRateCouponPricer> {
        let surface = ConstantOptionletVolatility::moving(
            0,
            self.calendar.clone(),
            BusinessDayConvention::Following,
            self.volatility,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            0.0,
            self.settings.clone(),
        );
        let handle = Handle::new(shared(surface) as Shared<dyn OptionletVolatilityStructure>);
        shared_mut(BlackIborCouponPricer::with_vol(handle))
            as SharedMut<dyn FloatingRateCouponPricer>
    }

    fn swap_npv(&self, fixed: Leg, floating: Leg) -> Real {
        let mut swap = Swap::two_leg(fixed, floating, self.settings.clone());
        let engine = shared_mut(DiscountingSwapEngine::new(
            self.curve.clone(),
            None,
            None,
            None,
            self.settings.clone(),
        ));
        swap.base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        swap.npv().unwrap()
    }

    fn cap_floor_npv(&self, cap_floor: QlResult<CapFloor>) -> Real {
        let mut cap_floor = cap_floor.unwrap();
        let vol = make_quote_handle(self.volatility).handle();
        let engine = shared_mut(
            BlackCapFloorEngine::with_flat_vol(
                self.curve.clone(),
                vol,
                Actual365Fixed::new(),
                0.0,
                self.settings.clone(),
            )
            .unwrap(),
        );
        cap_floor
            .base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        cap_floor.npv().unwrap()
    }
}

use crate::errors::QlResult;

fn erase(coupons: &[Shared<IborCoupon>]) -> Leg {
    coupons
        .iter()
        .map(|coupon| coupon.clone() as Shared<dyn CashFlow>)
        .collect()
}

/// `testLargeRates` (:188): a leg capped at 100 and floored at 0 is a no-op,
/// so its swap NPV matches the plain floating leg within 1e-10.
#[test]
fn a_degenerate_collar_matches_the_vanilla_leg() {
    let vars = Vars::new();
    let n = vars.length as usize;

    let fixed = vars.fixed_leg();
    let float_coupons = vars.float_coupons(1.0, 0.0);
    let vanilla = vars.swap_npv(fixed.clone(), erase(&float_coupons));
    let collared = vars.swap_npv(
        fixed,
        vars.capped_floored_leg(vec![100.0; n], vec![0.0; n], 1.0, 0.0),
    );

    assert!(
        (vanilla - collared).abs() < 1e-10,
        "vanilla {vanilla} vs collared {collared}"
    );
}

/// `testDecomposition` (:232): every capped/floored/collared leg equals the
/// vanilla leg plus or minus the matching cap/floor/collar instrument, at
/// gearing 1 and at positive and negative gearings with a spread, within
/// 1e-12.
#[test]
fn a_capped_floored_leg_matches_its_cap_floor_decomposition() {
    let vars = Vars::new();
    let n = vars.length as usize;
    let tol = 1e-12;
    let floor_strike = 0.05;
    let cap_strike = 0.10;
    let caps = vec![cap_strike; n];
    let floors = vec![floor_strike; n];
    let gearing_p = 0.5;
    let spread_p = 0.002;
    let gearing_n = -1.5;
    let spread_n = 0.12;

    let fixed = vars.fixed_leg();
    let float = vars.float_coupons(1.0, 0.0);
    let float_p = vars.float_coupons(gearing_p, spread_p);
    let float_n = vars.float_coupons(gearing_n, spread_n);
    let vanilla = vars.swap_npv(fixed.clone(), erase(&float));
    let vanilla_p = vars.swap_npv(fixed.clone(), erase(&float_p));
    let vanilla_n = vars.swap_npv(fixed.clone(), erase(&float_n));

    let settings = vars.settings.clone();
    let cap = |coupons: &[Shared<IborCoupon>], strike: Rate| {
        vars.cap_floor_npv(CapFloor::cap(
            coupons.to_vec(),
            vec![strike],
            settings.clone(),
        ))
    };
    let floor = |coupons: &[Shared<IborCoupon>], strike: Rate| {
        vars.cap_floor_npv(CapFloor::floor(
            coupons.to_vec(),
            vec![strike],
            settings.clone(),
        ))
    };
    let collar = |coupons: &[Shared<IborCoupon>], cap: Rate, floor: Rate| {
        vars.cap_floor_npv(CapFloor::collar(
            coupons.to_vec(),
            vec![cap],
            vec![floor],
            settings.clone(),
        ))
    };
    let cap_leg = |caps: Vec<Rate>, floors: Vec<Rate>, gearing, spread| {
        vars.swap_npv(
            fixed.clone(),
            vars.capped_floored_leg(caps, floors, gearing, spread),
        )
    };
    let close = |label: &str, got: Real, expected: Real| {
        assert!(
            (got - expected).abs() < tol,
            "{label}: {got} vs {expected} (diff {})",
            (got - expected).abs()
        );
    };

    close(
        "capped g=1",
        cap_leg(caps.clone(), Vec::new(), 1.0, 0.0),
        vanilla - cap(&float, cap_strike),
    );
    close(
        "floored g=1",
        cap_leg(Vec::new(), floors.clone(), 1.0, 0.0),
        vanilla + floor(&float, floor_strike),
    );
    close(
        "collared g=1",
        cap_leg(caps.clone(), floors.clone(), 1.0, 0.0),
        vanilla - collar(&float, cap_strike, floor_strike),
    );

    close(
        "capped g=0.5",
        cap_leg(caps.clone(), Vec::new(), gearing_p, spread_p),
        vanilla_p - cap(&float_p, cap_strike),
    );
    close(
        "capped g=-1.5",
        cap_leg(caps.clone(), Vec::new(), gearing_n, spread_n),
        vanilla_n + gearing_n * floor(&float, (cap_strike - spread_n) / gearing_n),
    );

    close(
        "floored g=0.5",
        cap_leg(Vec::new(), floors.clone(), gearing_p, spread_p),
        vanilla_p + floor(&float_p, floor_strike),
    );
    close(
        "floored g=-1.5",
        cap_leg(Vec::new(), floors.clone(), gearing_n, spread_n),
        vanilla_n - gearing_n * cap(&float, (floor_strike - spread_n) / gearing_n),
    );

    close(
        "collared g=0.5",
        cap_leg(caps.clone(), floors.clone(), gearing_p, spread_p),
        vanilla_p - collar(&float_p, cap_strike, floor_strike),
    );
    close(
        "collared g=-1.5",
        cap_leg(caps.clone(), floors.clone(), gearing_n, spread_n),
        vanilla_n
            - gearing_n
                * collar(
                    &float,
                    (floor_strike - spread_n) / gearing_n,
                    (cap_strike - spread_n) / gearing_n,
                ),
    );
}
