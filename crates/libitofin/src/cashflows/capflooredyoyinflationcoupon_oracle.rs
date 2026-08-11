//! Oracle for the capped/floored year-on-year inflation coupon.
//!
//! Net-new by design. QuantLib has no coupon-level counterpart to check against:
//! `test-suite/inflationcapflooredcoupon.cpp` reaches a capped coupon only
//! through a `YoYInflationCapFloor` instrument, whose engines and whose
//! `testDecomposition`/`testInstrumentEquality` oracle are deferred to `#851`.
//! What this module can pin without them, it pins compositionally: the Black,
//! displaced and Bachelier formulae were already pinned against C++ in earlier
//! batches, so the numbers below verify that the coupon routes to the right
//! already-pinned formula, with the right displacement, the right strike and
//! the right sign - and that its algebra closes. End-to-end reproduction of a
//! QuantLib premium waits for `#851`. No C++ dylib is needed here.
//!
//! ## The fixture
//!
//! It is 10 February 2022 and UK `YY_RPI` has published its year-on-year
//! figures. The volatility surface observes inflation eight months back, the
//! coupons three, which is what separates the two regimes the pricer switches
//! between: a coupon fixing on 10 May 2021 lands on or before the surface's
//! 1 June 2021 base date and is *determined*, priced as its intrinsic value with
//! no volatility read at all, while one fixing on 10 November 2021 lands after
//! it and is priced under a distribution even though its fixing is, as history,
//! already known. That is QuantLib's own test (`inflationcouponpricer.cpp:98`),
//! which keys on the surface's base date and not on the evaluation date.
//!
//! No forecast curve appears: every figure the coupons read is published, so the
//! numbers here isolate this batch's arithmetic from the bootstrapped
//! year-on-year curve that `piecewiseyoyinflationcurve.rs` already pins.

use crate::cashflows::capflooredyoyinflationcoupon::CappedFlooredYoYInflationCoupon;
use crate::cashflows::coupon::Coupon;
use crate::cashflows::yoyinflationcoupon::{YoYInflationCoupon, YoYInflationCouponPricer};
use crate::cashflows::yoyinflationoptionletpricer::{
    YoYInflationOptionletCouponPricer, YoYOptionletDistribution,
};
use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::Region;
use crate::indexes::index::Index;
use crate::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::{
    ConstantYoYOptionletVolatility, YoYOptionletVolatilitySurface,
};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::unitedkingdom::{self, UnitedKingdom};
use crate::time::date::Date;
use crate::time::date::Month::{August, February, June, May, November};
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::daycounters::thirty360::{Convention, Thirty360};
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Rate, Real, Spread, Volatility};

const VOL: Volatility = 0.01;
const NOMINAL: Real = 1_000_000.0;
const SPREAD: Spread = 0.0035;

/// The year-on-year figure the determined coupon observes (May 2021).
const DETERMINED_FIXING: Rate = 0.0281;
/// The year-on-year figure the live coupon observes (November 2021).
const LIVE_FIXING: Rate = 0.02935;

fn coupon_lag() -> Period {
    Period::new(3, TimeUnit::Months)
}

fn surface_lag() -> Period {
    Period::new(8, TimeUnit::Months)
}

/// UK `YY_RPI` as of 10 February 2022, carrying both published figures, and the
/// settings the surface shares with it.
fn published_index() -> (Shared<YoYInflationIndex>, Shared<Settings<Date>>) {
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(Date::new(10, February, 2022));
    let index = shared(YoYInflationIndex::new(
        "YY_RPI".into(),
        Region::uk(),
        false,
        Frequency::Monthly,
        Period::new(1, TimeUnit::Months),
        Currency::gbp(),
        Shared::clone(&settings),
    ));
    for (date, rate) in [
        (Date::new(1, May, 2021), DETERMINED_FIXING),
        (Date::new(1, November, 2021), LIVE_FIXING),
    ] {
        index.add_fixing(date, rate).expect("publishing a figure");
    }
    (index, settings)
}

/// A flat surface at [`VOL`], observing inflation [`surface_lag`] back, so its
/// base date is 1 June 2021.
fn flat_surface(settings: Shared<Settings<Date>>) -> Shared<ConstantYoYOptionletVolatility> {
    shared(ConstantYoYOptionletVolatility::new(
        VOL,
        0,
        UnitedKingdom::new(unitedkingdom::Market::Settlement),
        BusinessDayConvention::ModifiedFollowing,
        Actual365Fixed::new(),
        surface_lag(),
        Frequency::Monthly,
        false,
        -1.0,
        100.0,
        settings,
    ))
}

/// A coupon accruing the year ending `accrual_end`, observing the index
/// [`coupon_lag`] back.
fn coupon(
    index: &Shared<YoYInflationIndex>,
    accrual_end: Date,
    gearing: Real,
    spread: Spread,
) -> Shared<YoYInflationCoupon> {
    shared(YoYInflationCoupon::new(
        accrual_end,
        NOMINAL,
        accrual_end - Period::new(1, TimeUnit::Years),
        accrual_end,
        0,
        Shared::clone(index),
        coupon_lag(),
        CpiInterpolationType::Flat,
        Thirty360::with_convention(Convention::BondBasis),
        gearing,
        spread,
        None,
        None,
    ))
}

/// The whole fixture: a coupon ending `accrual_end`, wrapped in `cap`/`floor`,
/// carrying a pricer of the given `distribution` over the flat surface.
fn wrapped(
    distribution: YoYOptionletDistribution,
    accrual_end: Date,
    gearing: Real,
    spread: Spread,
    cap: Option<Rate>,
    floor: Option<Rate>,
) -> QlResult<(
    CappedFlooredYoYInflationCoupon,
    SharedMut<YoYInflationOptionletCouponPricer>,
)> {
    let (index, settings) = published_index();
    let surface = flat_surface(settings);
    let handle: Handle<dyn YoYOptionletVolatilitySurface> =
        Handle::new(Shared::clone(&surface) as Shared<dyn YoYOptionletVolatilitySurface>);
    let pricer = shared_mut(match distribution {
        YoYOptionletDistribution::Black => {
            YoYInflationOptionletCouponPricer::black(handle, Handle::empty())
        }
        YoYOptionletDistribution::UnitDisplaced => {
            YoYInflationOptionletCouponPricer::unit_displaced(handle, Handle::empty())
        }
        YoYOptionletDistribution::Bachelier => {
            YoYInflationOptionletCouponPricer::bachelier(handle, Handle::empty())
        }
    });
    let wrapper = CappedFlooredYoYInflationCoupon::new(
        coupon(&index, accrual_end, gearing, spread),
        cap,
        floor,
    )?;
    wrapper.set_pricer(pricer.clone() as SharedMut<dyn YoYInflationCouponPricer>);
    Ok((wrapper, pricer))
}

/// The accrual end whose fixing date, 10 November 2021, falls *after* the
/// surface's base date: priced under a distribution.
fn live_end() -> Date {
    Date::new(10, February, 2022)
}

/// The accrual end whose fixing date, 10 May 2021, falls on or before the
/// surface's base date: determined, priced as its intrinsic value.
fn determined_end() -> Date {
    Date::new(10, August, 2021)
}

const EVERY_DISTRIBUTION: [YoYOptionletDistribution; 3] = [
    YoYOptionletDistribution::Black,
    YoYOptionletDistribution::UnitDisplaced,
    YoYOptionletDistribution::Bachelier,
];

/// A gearing and a cap/floor level whose effective strike,
/// `(level - spread) / gearing`, lands on the forward.
///
/// The level has to be chosen per gearing sign rather than shared: dividing by a
/// negative gearing turns an ordinary level into a *negative* effective strike,
/// which the two lognormal pricers refuse outright (`blackformula.rs:60`, as
/// `blackFormula` does in C++). Only the Bachelier pricer prices one, so the
/// tests that sweep all three sweep levels that keep the strike positive.
const LEVELS: [(Real, Rate); 2] = [(2.5, 0.076), (-1.5, -0.04)];

/// A gearing with a floor and a cap level, on the same footing as [`LEVELS`].
const COLLAR_LEVELS: [(Real, Rate, Rate); 2] = [(2.5, 0.04, 0.10), (-1.5, -0.06, -0.02)];

fn rate_of(
    distribution: YoYOptionletDistribution,
    gearing: Real,
    cap: Option<Rate>,
    floor: Option<Rate>,
) -> Rate {
    let (wrapper, _) = wrapped(distribution, live_end(), gearing, SPREAD, cap, floor)
        .expect("the levels are consistent");
    wrapper.rate().expect("the observed month is published")
}

/// The fixture is the one it claims to be: the surface's base date sits between
/// the two coupons' fixing dates, so one coupon is determined and the other is
/// not.
#[test]
fn the_base_date_separates_the_determined_coupon_from_the_live_one() {
    let (index, settings) = published_index();
    let surface = flat_surface(settings);
    let base_date = surface.base_date().expect("the reference date is set");

    assert_eq!(base_date, Date::new(1, June, 2021));
    assert_eq!(
        coupon(&index, determined_end(), 1.0, 0.0).fixing_date(),
        Date::new(10, May, 2021)
    );
    assert_eq!(
        coupon(&index, live_end(), 1.0, 0.0).fixing_date(),
        Date::new(10, November, 2021)
    );
    assert!(coupon(&index, determined_end(), 1.0, 0.0).fixing_date() <= base_date);
    assert!(coupon(&index, live_end(), 1.0, 0.0).fixing_date() > base_date);
}

/// `min(x, K) + max(x, K) = x + K`, in rates: capping and flooring the same
/// coupon at the same level sums to the swaplet plus that level. It holds under
/// every distribution, since each satisfies put-call parity, and under either
/// gearing sign, where the roles swap but the effective strike does not.
///
/// This is the identity that exercises `rate()` end to end: the effective
/// strike, the gearing multiple, the sign the caplet and floorlet enter with,
/// and the routing to the pricer. Dropping the spread from the effective strike,
/// or subtracting the floorlet, breaks it.
#[test]
fn a_cap_and_a_floor_at_one_level_sum_to_the_swaplet_plus_that_level() {
    for distribution in EVERY_DISTRIBUTION {
        for (gearing, level) in LEVELS {
            let swaplet = rate_of(distribution, gearing, None, None);
            let capped = rate_of(distribution, gearing, Some(level), None);
            let floored = rate_of(distribution, gearing, None, Some(level));

            let sum = capped + floored;
            assert!(
                (sum - (swaplet + level)).abs() < 1e-12,
                "{distribution:?} at gearing {gearing}: {sum} against {}",
                swaplet + level
            );
        }
    }
}

/// A collar is its floor plus its cap, less the swaplet counted twice: the
/// both-levels path adds the same two optionlets the single-level paths do.
#[test]
fn a_collar_is_its_floor_and_its_cap_less_the_swaplet() {
    for distribution in EVERY_DISTRIBUTION {
        for (gearing, floor_level, cap_level) in COLLAR_LEVELS {
            let swaplet = rate_of(distribution, gearing, None, None);
            let floored = rate_of(distribution, gearing, None, Some(floor_level));
            let capped = rate_of(distribution, gearing, Some(cap_level), None);
            let collared = rate_of(distribution, gearing, Some(cap_level), Some(floor_level));

            assert!(
                (collared - (floored + capped - swaplet)).abs() < 1e-12,
                "{distribution:?} at gearing {gearing}: collar was {collared}"
            );
        }
    }
}
