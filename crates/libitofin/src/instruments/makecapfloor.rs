//! Standard market cap/floor builder (`MakeCapFloor`).
//!
//! Port of `ql/instruments/makecapfloor.{hpp,cpp}`: the comfortable way to
//! instantiate a standard market [`CapFloor`]. It derives a floating leg from a
//! cap/floor tenor, an [`IborIndex`] and a forward start by delegating to
//! [`MakeVanillaSwap`], keeps only that leg (the C++ `operator CapFloor()` builds
//! the whole swap and reads `VanillaSwap::floatingLeg()`,
//! `makecapfloor.cpp:48-62`), drops the spot caplet when the forward start is
//! zero, and wraps the result in a [`CapFloor`] of the requested type.
//!
//! ## The dropped spot caplet
//!
//! When `forward_start == 0*Days` the constructor sets `firstCapletExcluded`
//! (`makecapfloor.cpp:34`) and the conversion erases the first coupon
//! (`makecapfloor.cpp:52-54`). The drop is load-bearing: the optionlet stripper
//! and its oracle build spot-start caps, and dropping the first caplet is how
//! QuantLib avoids needing a historical index fixing at the evaluation date,
//! which matches the D5/D11 explicit-fixings design. A non-zero forward start
//! keeps every coupon.
//!
//! ## The dummy fixed leg
//!
//! The delegated builder is `MakeVanillaSwap(tenor, index, 0.0, forward_start)`
//! with a `1*Years` / `Actual365Fixed` fixed leg. In C++ that fixed leg exists
//! only so `MakeVanillaSwap` does not throw on an unknown fixed-leg currency
//! default; only the floating leg is ever used. The Rust
//! [`MakeVanillaSwap::floating_leg`] never consults the fixed-leg defaults, so
//! the two `with_fixed_leg_*` calls are inert here, but they are kept to mirror
//! the C++ construction verbatim.
//!
//! ## Divergences from QuantLib
//!
//! - The strike is a required [`Rate`]; the C++ `Null<Rate>` sentinel that
//!   triggers an ATM fill off the engine's discount curve
//!   (`makecapfloor.cpp:63-88`) is deferred to #577.
//! - Only [`with_pricing_engine`](Self::with_pricing_engine) is ported. The other
//!   `with*` knobs (`withNominal`, `withEffectiveDate`, `withTenor`, the schedule
//!   overrides) and `asOptionlet` are omitted: the optionlet stripper and its
//!   oracle use none of them.
//! - `CapFloor::Type::Collar` has no single-strike form and `MakeCapFloor` is
//!   only ever built for a cap or a floor, so a collar type returns an error.

use crate::errors::QlResult;
use crate::indexes::IborIndex;
use crate::instrument::Instrument;
use crate::instruments::capfloor::{CapFloor, CapFloorType};
use crate::instruments::makevanillaswap::MakeVanillaSwap;
use crate::pricingengine::PricingEngine;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::Rate;

/// Builder for a standard market cap or floor.
pub struct MakeCapFloor {
    cap_floor_type: CapFloorType,
    strike: Rate,
    first_caplet_excluded: bool,
    make_vanilla_swap: MakeVanillaSwap,
    settings: Shared<Settings<Date>>,
    engine: Option<SharedMut<dyn PricingEngine>>,
}

impl MakeCapFloor {
    /// Starts a builder for a `cap_floor_type` cap/floor of `cap_floor_tenor` on
    /// `ibor_index`, struck at `strike`, starting `forward_start` after spot
    /// (`makecapfloor.cpp:29-40`).
    ///
    /// A zero `forward_start` marks the spot caplet for exclusion; see the module
    /// docs. `settings` carries the evaluation date (D5).
    pub fn new(
        cap_floor_type: CapFloorType,
        cap_floor_tenor: Period,
        ibor_index: Shared<IborIndex>,
        strike: Rate,
        forward_start: Period,
        settings: Shared<Settings<Date>>,
    ) -> MakeCapFloor {
        let first_caplet_excluded = forward_start == Period::new(0, TimeUnit::Days);
        let make_vanilla_swap = MakeVanillaSwap::new(
            cap_floor_tenor,
            Shared::clone(&ibor_index),
            Some(0.0),
            forward_start,
            Shared::clone(&settings),
        )
        .with_fixed_leg_tenor(Period::new(1, TimeUnit::Years))
        .with_fixed_leg_day_count(Actual365Fixed::new());

        MakeCapFloor {
            cap_floor_type,
            strike,
            first_caplet_excluded,
            make_vanilla_swap,
            settings,
            engine: None,
        }
    }

    /// Sets the engine installed on the built cap/floor
    /// (`makecapfloor.cpp:withPricingEngine`).
    pub fn with_pricing_engine(mut self, engine: SharedMut<dyn PricingEngine>) -> MakeCapFloor {
        self.engine = Some(engine);
        self
    }

    /// Builds the cap/floor (C++ `operator CapFloor()` /
    /// `operator shared_ptr<CapFloor>()`, `makecapfloor.cpp:42-90`).
    ///
    /// Derives the floating leg through [`MakeVanillaSwap::floating_leg`], drops
    /// the spot caplet when [`new`](Self::new) marked it excluded, wraps the leg
    /// in a [`CapFloor`] of the requested type struck at the single strike, and
    /// installs the pricing engine when one was set.
    ///
    /// # Errors
    ///
    /// Propagates the floating-leg derivation, rejects a collar type (no
    /// single-strike form), and propagates the [`CapFloor`] construction.
    pub fn build(self) -> QlResult<CapFloor> {
        let mut coupons = self.make_vanilla_swap.floating_leg()?;
        if self.first_caplet_excluded && !coupons.is_empty() {
            coupons.remove(0);
        }

        let strikes = vec![self.strike];
        let mut cap_floor = match self.cap_floor_type {
            CapFloorType::Cap => CapFloor::cap(coupons, strikes, Shared::clone(&self.settings))?,
            CapFloorType::Floor => {
                CapFloor::floor(coupons, strikes, Shared::clone(&self.settings))?
            }
            CapFloorType::Collar => {
                crate::fail!("MakeCapFloor builds only caps and floors, not collars")
            }
        };

        if let Some(engine) = self.engine {
            cap_floor.base_mut().set_pricing_engine(engine);
        }
        Ok(cap_floor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::Coupon;
    use crate::event::Event;
    use crate::handle::Handle;
    use crate::indexes::ibor::Euribor;
    use crate::shared::shared;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::Month;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn euribor6m(settings: Shared<Settings<Date>>) -> Shared<IborIndex> {
        shared(Euribor::six_months(
            Handle::<dyn YieldTermStructure>::empty(),
            settings,
        ))
    }

    /// The reference is the undropped floating leg the same builder derives, so
    /// the only difference from the make-built cap is the excluded spot caplet.
    /// This isolates the `firstCapletExcluded` drop, the one behaviour #576's
    /// two-engine round-trip cannot catch (a construction bug cancels there).
    #[test]
    fn a_spot_cap_drops_the_first_caplet() {
        let settings = settings_on(Date::new(15, Month::January, 2026));
        let index = euribor6m(settings.clone());
        let tenor = Period::new(3, TimeUnit::Years);

        let reference = MakeVanillaSwap::new(
            tenor,
            Shared::clone(&index),
            Some(0.0),
            Period::new(0, TimeUnit::Days),
            settings.clone(),
        )
        .floating_leg()
        .unwrap();

        let cap = MakeCapFloor::new(
            CapFloorType::Cap,
            tenor,
            index,
            0.03,
            Period::new(0, TimeUnit::Days),
            settings,
        )
        .build()
        .unwrap();

        assert_eq!(cap.coupons().len(), reference.len() - 1);
        assert_eq!(cap.coupons()[0].date(), reference[1].date());
        assert_eq!(
            cap.coupons()[0].accrual_start_date(),
            reference[1].accrual_start_date()
        );

        let last = cap.last_floating_rate_coupon().unwrap();
        let reference_last = reference.last().unwrap();
        assert_eq!(last.date(), reference_last.date());
        assert_eq!(last.fixing_date(), reference_last.fixing_date());
        assert_eq!(last.accrual_period(), reference_last.accrual_period());
    }

    /// A non-zero forward start keeps every coupon: the drop is conditional
    /// (`makecapfloor.cpp:34`).
    #[test]
    fn a_forward_starting_cap_keeps_the_first_caplet() {
        let settings = settings_on(Date::new(15, Month::January, 2026));
        let index = euribor6m(settings.clone());
        let tenor = Period::new(3, TimeUnit::Years);
        let forward_start = Period::new(6, TimeUnit::Months);

        let reference = MakeVanillaSwap::new(
            tenor,
            Shared::clone(&index),
            Some(0.0),
            forward_start,
            settings.clone(),
        )
        .floating_leg()
        .unwrap();

        let cap = MakeCapFloor::new(
            CapFloorType::Cap,
            tenor,
            index,
            0.03,
            forward_start,
            settings,
        )
        .build()
        .unwrap();

        assert_eq!(cap.coupons().len(), reference.len());
        assert_eq!(cap.coupons()[0].date(), reference[0].date());
    }

    /// A floor is built with the strike routed to the floor vector.
    #[test]
    fn a_floor_carries_the_strike_as_a_floor_rate() {
        let settings = settings_on(Date::new(15, Month::January, 2026));
        let index = euribor6m(settings.clone());
        let floor = MakeCapFloor::new(
            CapFloorType::Floor,
            Period::new(2, TimeUnit::Years),
            index,
            0.01,
            Period::new(0, TimeUnit::Days),
            settings,
        )
        .build()
        .unwrap();

        assert_eq!(floor.cap_floor_type(), CapFloorType::Floor);
        assert!(floor.cap_rates().is_empty());
        assert_eq!(floor.floor_rates()[0], 0.01);
    }
}
