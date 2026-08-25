//! Bootstrap helpers for the year-on-year optionlet volatility bootstrap.
//!
//! Port of `ql/experimental/inflation/yoyoptionlethelpers.{hpp,cpp}`:
//! [`YoYOptionletHelper`] is `BootstrapHelper<YoYOptionletVolatilitySurface>`
//! over a quoted cap/floor *price* (`hpp:35-67`), repricing one
//! [`YoYInflationCapFloor`] per pillar. [`YoYOptionletVolatilityHelper`] is
//! the family trait, split off the C++ template exactly as
//! [`YoYInflationHelper`] is on the inflation-curve side.
//!
//! ## The engine repointing divergence
//!
//! C++ `setTermStructure` wraps the curve being bootstrapped in a non-owning
//! handle and calls `pricer_->setVolatility(volSurf)` (`cpp:74-89`), mutating
//! the shared engine's member. The Rust engine holds its volatility
//! [`Handle`] immutably, so the repointing is expressed through the *retained
//! shared link*: the helper is handed the [`RelinkableHandle`] the engine was
//! built over and re-points it - weakly, the `null_deleter` analogue - at each
//! curve it is handed. The relink also notifies the engine, which C++'s
//! member overwrite never did; the C++ side compensates with the `deepUpdate`
//! in `impliedQuote` (`cpp:68-71`), which is ported too, because a bootstrap
//! moving a curve *node* (rather than the handle) notifies nobody.
//!
//! [`YoYInflationHelper`]: crate::termstructures::inflation::inflationhelpers::YoYInflationHelper

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
use crate::instrument::Instrument;
use crate::instruments::{CapFloorType, MakeYoYInflationCapFloor, YoYInflationCapFloor};
use crate::patterns::observable::AsObservable;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::inflation::YoYInflationCapFloorEngine;
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::termstructures::bootstraphelper::{BootstrapHelperBase, BootstrapHelperShared};
use crate::termstructures::volatility::YoYOptionletVolatilitySurface;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::types::{Natural, Rate, Real, Size};

/// The shared state of a year-on-year optionlet volatility bootstrap helper: a
/// [`BootstrapHelperBase`] whose back-pointer is a volatility surface.
pub type YoYOptionletVolHelperBase = BootstrapHelperBase<dyn YoYOptionletVolatilitySurface>;

/// Bootstrap helper family for the year-on-year optionlet volatility bootstrap
/// (`BootstrapHelper<YoYOptionletVolatilitySurface>`).
pub trait YoYOptionletVolatilityHelper: AsObservable {
    /// The embedded shared state.
    fn base(&self) -> &YoYOptionletVolHelperBase;

    /// The price implied by the current surface, computed by the concrete
    /// helper.
    fn implied_quote(&self) -> QlResult<Real>;

    /// The market price the helper fits the surface to.
    fn quote(&self) -> &Handle<dyn Quote> {
        self.base().quote()
    }

    /// The bootstrap's root: market price minus implied price.
    fn quote_error(&self) -> QlResult<Real> {
        Ok(self.base().quote_value()? - self.implied_quote()?)
    }

    /// Sets the surface being bootstrapped (non-owning, unobserved).
    fn set_term_structure(&self, term_structure: &Shared<dyn YoYOptionletVolatilitySurface>) {
        self.base().set_term_structure(term_structure);
    }

    /// The earliest date data are needed at.
    fn earliest_date(&self) -> Date {
        self.base().earliest_date()
    }

    /// The instrument's maturity date.
    fn maturity_date(&self) -> Date {
        self.base().maturity_date()
    }

    /// The latest date data are needed at.
    fn latest_relevant_date(&self) -> Date {
        self.base().latest_relevant_date()
    }

    /// The pillar date, at which the surface node this helper sets sits.
    fn pillar_date(&self) -> Date {
        self.base().pillar_date()
    }

    /// The latest date, equal to the pillar date.
    fn latest_date(&self) -> Date {
        self.base().latest_date()
    }
}

/// The volatility half of the driver bound, routing through
/// [`YoYOptionletVolatilityHelper`] so a concrete helper's
/// `set_term_structure` override still runs.
impl BootstrapHelperShared for dyn YoYOptionletVolatilityHelper {
    type TS = dyn YoYOptionletVolatilitySurface;

    fn set_term_structure(&self, term_structure: &Shared<dyn YoYOptionletVolatilitySurface>) {
        YoYOptionletVolatilityHelper::set_term_structure(self, term_structure);
    }

    fn quote_value(&self) -> QlResult<Real> {
        self.base().quote_value()
    }

    fn quote_error(&self) -> QlResult<Real> {
        YoYOptionletVolatilityHelper::quote_error(self)
    }

    fn pillar_date(&self) -> Date {
        YoYOptionletVolatilityHelper::pillar_date(self)
    }

    fn latest_relevant_date(&self) -> Date {
        YoYOptionletVolatilityHelper::latest_relevant_date(self)
    }

    fn maturity_date(&self) -> Date {
        YoYOptionletVolatilityHelper::maturity_date(self)
    }
}

/// Year-on-year inflation-volatility bootstrap helper (`YoYOptionletHelper`,
/// `hpp:35-67`): reprices one cap/floor, built once at construction
/// (`cpp:44-51`), against the surface being bootstrapped.
///
/// C++'s stored configuration members (`hpp:55-64`) exist only to build that
/// instrument, so here they reduce to it; what survives is the contract and
/// the retained volatility link of the module docs.
pub struct YoYOptionletHelper {
    base: YoYOptionletVolHelperBase,
    vol_handle: RelinkableHandle<dyn YoYOptionletVolatilitySurface>,
    capfloor: RefCell<YoYInflationCapFloor>,
}

impl YoYOptionletHelper {
    /// Builds the helper and its cap/floor (`cpp:27-65`): `n` annual payments
    /// on `index` observed `lag` back, struck at `strike` on a notional of
    /// `notional` ("get the price level right, e.g., bps = 10,000", `hpp:39`),
    /// with `pricer` installed on it. The helper's earliest and latest dates
    /// are the fixing dates of the leg's first and last coupons (`cpp:53-59`);
    /// the pillar falls back to the latest, C++'s default.
    ///
    /// `vol_handle` must be the link `pricer` reads its volatility through;
    /// see the module docs. `settings` carries the evaluation date the leg
    /// starts at (D5).
    ///
    /// # Errors
    ///
    /// As `MakeYoYInflationCapFloor::build`: no evaluation date, or an
    /// unbuildable leg.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        price: Handle<dyn Quote>,
        notional: Real,
        cap_floor_type: CapFloorType,
        lag: Period,
        yoy_day_counter: DayCounter,
        payment_calendar: Calendar,
        fixing_days: Natural,
        index: &Shared<YoYInflationIndex>,
        interpolation: CpiInterpolationType,
        strike: Rate,
        n: Size,
        pricer: SharedMut<YoYInflationCapFloorEngine>,
        vol_handle: RelinkableHandle<dyn YoYOptionletVolatilitySurface>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Shared<YoYOptionletHelper>> {
        let capfloor = MakeYoYInflationCapFloor::new(
            cap_floor_type,
            Shared::clone(index),
            n,
            payment_calendar,
            lag,
            interpolation,
            settings,
        )
        .with_nominal(notional)
        .with_fixing_days(fixing_days)
        .with_payment_day_counter(yoy_day_counter)
        .with_strike(strike)
        .with_pricing_engine(pricer as SharedMut<dyn PricingEngine>)
        .build()?;

        let base = YoYOptionletVolHelperBase::new(price);
        let leg = capfloor.yoy_leg();
        base.set_earliest_date(leg.first().expect("the leg is non-empty").fixing_date());
        base.set_latest_date(leg.last().expect("the leg is non-empty").fixing_date());

        Ok(crate::shared::shared(YoYOptionletHelper {
            base,
            vol_handle,
            capfloor: RefCell::new(capfloor),
        }))
    }

    /// The cap/floor the helper reprices.
    pub fn capfloor(&self) -> &RefCell<YoYInflationCapFloor> {
        &self.capfloor
    }
}

impl AsObservable for YoYOptionletHelper {
    fn observable(&self) -> &crate::patterns::observable::Observable {
        self.base.observable()
    }
}

impl YoYOptionletVolatilityHelper for YoYOptionletHelper {
    fn base(&self) -> &YoYOptionletVolHelperBase {
        &self.base
    }

    /// The cap/floor's NPV after a forced refresh (`impliedQuote`,
    /// `cpp:68-71`). The `deepUpdate` is load-bearing: the bootstrap moves the
    /// surface's nodes directly, which notifies nobody, so the cached NPV
    /// would otherwise stand.
    fn implied_quote(&self) -> QlResult<Real> {
        let mut capfloor = self.capfloor.borrow_mut();
        capfloor.base().observer().borrow_mut().update();
        capfloor.npv()
    }

    /// Re-points the retained volatility link at the surface under
    /// construction - weakly, so the surface is neither owned nor observed -
    /// then records it (`setTermStructure`, `cpp:74-89`).
    fn set_term_structure(&self, term_structure: &Shared<dyn YoYOptionletVolatilitySurface>) {
        self.base.set_term_structure(term_structure);
        self.vol_handle
            .link_to_weak(Shared::downgrade(term_structure));
    }
}

#[cfg(test)]
mod tests {
    //! The helper's numeric proof closes with the stripper's oracle; pinned
    //! here are the decisions it makes on its own: which dates it derives from
    //! the leg it builds, and how `set_term_structure` re-points the retained
    //! volatility link.

    use super::*;
    use crate::currency::Currency;
    use crate::indexes::inflation::YyGenericCpi;
    use crate::quotes::SimpleQuote;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::volatility::ConstantYoYOptionletVolatility;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month::{June, March};
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::timeunit::TimeUnit;

    fn lag() -> Period {
        Period::new(3, TimeUnit::Months)
    }

    struct Fixture {
        settings: Shared<Settings<Date>>,
        vol_handle: RelinkableHandle<dyn YoYOptionletVolatilitySurface>,
        helper: Shared<YoYOptionletHelper>,
    }

    /// A three-payment cap helper on a generic index, its engine reading the
    /// retained link, which starts empty as the stripper's does.
    fn a_helper() -> Fixture {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(15, June, 2026));
        let index = shared(YyGenericCpi::new(
            Frequency::Monthly,
            false,
            lag(),
            Currency::eur(),
            Shared::clone(&settings),
        ));
        let vol_handle = RelinkableHandle::<dyn YoYOptionletVolatilitySurface>::empty();
        let pricer = shared_mut(YoYInflationCapFloorEngine::unit_displaced(
            Shared::clone(&index),
            vol_handle.handle(),
            crate::handle::Handle::empty(),
        ));
        let helper = YoYOptionletHelper::new(
            Handle::new(shared(SimpleQuote::new(Some(25.0)))),
            10_000.0,
            CapFloorType::Cap,
            lag(),
            Actual365Fixed::new(),
            Target::new(),
            0,
            &index,
            CpiInterpolationType::Flat,
            0.03,
            3,
            pricer,
            vol_handle.clone(),
            Shared::clone(&settings),
        )
        .expect("a well-formed helper");
        Fixture {
            settings,
            vol_handle,
            helper,
        }
    }

    /// The dates come from the leg (`cpp:53-59`): the first and last coupons'
    /// fixing dates, each the reference-period end less the 3-month lag, and
    /// the pillar falls back to the latest.
    #[test]
    fn the_dates_are_the_first_and_last_coupon_fixings() {
        let fixture = a_helper();
        let helper = &fixture.helper;

        assert_eq!(helper.earliest_date(), Date::new(15, March, 2027));
        assert_eq!(helper.latest_date(), Date::new(15, March, 2029));
        assert_eq!(helper.pillar_date(), Date::new(15, March, 2029));
        assert_eq!(helper.capfloor().borrow().yoy_leg().len(), 3);
        let _ = fixture.settings;
    }

    /// `set_term_structure` re-points the retained link at the surface under
    /// construction, weakly: every copy of the link sees it, and dropping the
    /// surface empties the link rather than leaking a kept-alive curve.
    #[test]
    fn set_term_structure_repoints_the_retained_link_weakly() {
        let fixture = a_helper();
        assert!(fixture.vol_handle.handle().current_link().is_err());

        let surface = shared(ConstantYoYOptionletVolatility::new(
            0.01,
            0,
            Target::new(),
            BusinessDayConvention::ModifiedFollowing,
            Actual365Fixed::new(),
            lag(),
            Frequency::Monthly,
            false,
            -1.0,
            3.0,
            Shared::clone(&fixture.settings),
        )) as Shared<dyn YoYOptionletVolatilitySurface>;

        YoYOptionletVolatilityHelper::set_term_structure(fixture.helper.as_ref(), &surface);
        assert!(
            Shared::ptr_eq(
                &fixture.vol_handle.handle().current_link().unwrap(),
                &surface
            ),
            "the link must point at the surface just set"
        );
        assert!(fixture.helper.base().term_structure().is_ok());

        drop(surface);
        assert!(
            fixture.vol_handle.handle().current_link().is_err(),
            "the link must not keep the surface alive"
        );
    }
}
