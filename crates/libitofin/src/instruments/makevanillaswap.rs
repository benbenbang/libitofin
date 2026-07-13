//! Vanilla swap builder (`MakeVanillaSwap`).
//!
//! Port of `ql/instruments/makevanillaswap.{hpp,cpp}`: `class MakeVanillaSwap`,
//! the comfortable way to instantiate a standard market [`VanillaSwap`]. It
//! derives the start and end dates, the two schedules and the discounting engine
//! from a swap tenor, an [`IborIndex`] and a handful of overrides, then hands
//! them to [`VanillaSwap::new`] and attaches a [`DiscountingSwapEngine`]. C++'s
//! `operator VanillaSwap()` / `operator shared_ptr<VanillaSwap>()` become
//! [`MakeVanillaSwap::build`], which returns the priced swap.
//!
//! This is the class `SwapRateHelper::initializeDates` builds its swap through
//! (`ratehelpers.cpp:551-570`) rather than the [`VanillaSwap`] ctor, which takes
//! prebuilt schedules and cannot derive them from a tenor. The bootstrap oracle
//! also builds through it (`piecewiseyieldcurve.cpp:384-388`).
//!
//! ## Ported knobs
//!
//! The builder exposes the overrides `SwapRateHelper` and the bootstrap oracle
//! use (the `with_*` methods below, each documented with its C++ cite): the
//! effective / termination / settlement dates, the nominal, the fixed and
//! floating leg calendars / conventions / termination-date conventions /
//! end-of-month flags, the fixed-leg tenor and day count, the discounting term
//! structure and the indexed-coupon mode. The swap tenor, index, optional fixed
//! rate and forward start are the constructor arguments (`makevanillaswap.hpp:41`).
//!
//! ## Deferred knobs
//!
//! Every other `with*` on `makevanillaswap.hpp` is deferred, defaulting to its
//! C++ default (`makevanillaswap.hpp:95-116`):
//!
//! - swap type (`receiveFixed` / `withType`): defaults to `Payer`;
//! - `withRule` / `withFixedLegRule` / `withFloatingLegRule`: the schedules use
//!   `DateGeneration::Backward`;
//! - `withPaymentConvention`: unset, so [`VanillaSwap::new`] resolves the payment
//!   convention against the floating schedule;
//! - `withFixedLegFirstDate` / `withFixedLegNextToLastDate` /
//!   `withFloatingLegFirstDate` / `withFloatingLegNextToLastDate`: the stub dates
//!   default to null;
//! - `withFloatingLegTenor` / `withFloatingLegDayCount`: taken from the index
//!   (`makevanillaswap.cpp:46/50`);
//! - `withFloatingLegSpread`: defaults to `0.0`;
//! - `withMaturityEndOfMonth`: defaults to the floating-leg end-of-month flag
//!   (`makevanillaswap.cpp:98`);
//! - `withPricingEngine`: the engine is always the [`DiscountingSwapEngine`] over
//!   the discounting curve (set) or the index's forwarding curve (default),
//!   matching `makevanillaswap.cpp:171-199`.
//!
//! ## Fixed-leg currency defaults (`makevanillaswap.cpp:104-163`)
//!
//! When [`with_fixed_leg_tenor`](Self::with_fixed_leg_tenor) /
//! [`with_fixed_leg_day_count`](Self::with_fixed_leg_day_count) are unset, C++
//! infers both from the index currency. The port's [`Currency`] carries only
//! `EUR` and `USD`, so only their branches are expressible: fixed tenor `1Y` for
//! both, day count `Thirty360(BondBasis)` for EUR and `Actual360` for USD. Any
//! other currency returns an error, exactly as C++'s `QL_FAIL` does for an
//! unrecognised currency. The tenor-length-dependent branches (GBP/JPY/AUD/HKD)
//! are deferred with those currencies. Neither `SwapRateHelper` nor the bootstrap
//! oracle hits this fallback: both always set the fixed tenor and day count
//! explicitly.
//!
//! ## `with_indexed_coupons` and D5
//!
//! C++ threads an `optional<bool>` into the leg builder to force indexed or
//! at-par coupons per swap. In this crate that mode lives on
//! [`Settings::using_at_par_coupons`] (#315), and [`VanillaSwap::new`]
//! deliberately defers the per-swap override (its module doc). So the maker
//! cannot thread the flag into the swap. Rather than silently ignore a passed
//! value, [`with_indexed_coupons`](Self::with_indexed_coupons) records the
//! request and [`build`](Self::build) refuses it when it conflicts with the
//! current [`Settings`] mode (`Some(true)` demands indexed coupons, i.e.
//! `!using_at_par_coupons()`); a request that agrees, or `None`, is accepted and
//! the Settings mode drives the coupons. The builder never mutates [`Settings`].

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::swap::SwapType;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::DiscountingSwapEngine;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual360::Actual360;
use crate::time::daycounters::thirty360::{Convention, Thirty360};
use crate::time::period::Period;
use crate::time::schedule::{Schedule, allows_end_of_month};
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real};

use super::VanillaSwap;

/// Builder for a [`VanillaSwap`] (`ql/instruments/makevanillaswap.hpp`).
///
/// Construct with [`new`](Self::new), chain the ported `with_*` overrides, then
/// [`build`](Self::build) to get the priced swap.
pub struct MakeVanillaSwap {
    swap_tenor: Period,
    ibor_index: Shared<IborIndex>,
    fixed_rate: Option<Rate>,
    forward_start: Period,
    settings: Shared<Settings<Date>>,

    settlement_days: Option<Natural>,
    effective_date: Option<Date>,
    termination_date: Option<Date>,
    nominal: Real,

    fixed_calendar: Calendar,
    float_calendar: Calendar,
    fixed_tenor: Option<Period>,
    fixed_convention: BusinessDayConvention,
    fixed_termination_date_convention: BusinessDayConvention,
    float_convention: BusinessDayConvention,
    float_termination_date_convention: BusinessDayConvention,
    fixed_end_of_month: bool,
    float_end_of_month: bool,
    fixed_day_count: Option<DayCounter>,

    use_indexed_coupons: Option<bool>,
    discounting_curve: Option<Handle<dyn YieldTermStructure>>,
}

impl MakeVanillaSwap {
    /// Starts a builder for a swap of `swap_tenor` on `ibor_index`
    /// (`makevanillaswap.cpp:40`).
    ///
    /// `fixed_rate` is the C++ `Null<Rate>()`-defaulted fixed rate: `Some(r)`
    /// fixes the leg at `r`, `None` fills it with the fair rate at build time
    /// (`makevanillaswap.cpp:165-185`). `forward_start` is the C++
    /// `0*Days`-defaulted forward start. `settings` carries the evaluation date
    /// (D5). The float calendar, tenor, conventions and day count default from
    /// the index (`makevanillaswap.cpp:45-50`).
    pub fn new(
        swap_tenor: Period,
        ibor_index: Shared<IborIndex>,
        fixed_rate: Option<Rate>,
        forward_start: Period,
        settings: Shared<Settings<Date>>,
    ) -> MakeVanillaSwap {
        let fixing_calendar = ibor_index.fixing_calendar();
        let index_convention = ibor_index.business_day_convention();
        MakeVanillaSwap {
            swap_tenor,
            ibor_index,
            fixed_rate,
            forward_start,
            settings,
            settlement_days: None,
            effective_date: None,
            termination_date: None,
            nominal: 1.0,
            fixed_calendar: fixing_calendar.clone(),
            float_calendar: fixing_calendar,
            fixed_tenor: None,
            fixed_convention: BusinessDayConvention::ModifiedFollowing,
            fixed_termination_date_convention: BusinessDayConvention::ModifiedFollowing,
            float_convention: index_convention,
            float_termination_date_convention: index_convention,
            fixed_end_of_month: false,
            float_end_of_month: false,
            fixed_day_count: None,
            use_indexed_coupons: None,
            discounting_curve: None,
        }
    }

    /// Sets the swap's start date explicitly, bypassing the settlement-days spot
    /// derivation (`makevanillaswap.cpp:225`).
    pub fn with_effective_date(mut self, effective_date: Date) -> MakeVanillaSwap {
        self.effective_date = Some(effective_date);
        self
    }

    /// Sets an explicit maturity, clearing the swap tenor so the currency-default
    /// inference uses the actual length (`makevanillaswap.cpp:231`).
    pub fn with_termination_date(mut self, termination_date: Date) -> MakeVanillaSwap {
        self.termination_date = Some(termination_date);
        self.swap_tenor = Period::new(0, TimeUnit::Days);
        self
    }

    /// Sets the number of days from the evaluation date to the spot start date,
    /// used only when no effective date is set (`makevanillaswap.cpp:219`).
    pub fn with_settlement_days(mut self, settlement_days: Natural) -> MakeVanillaSwap {
        self.settlement_days = Some(settlement_days);
        self
    }

    /// Sets the nominal shared by both legs (`makevanillaswap.cpp:214`).
    pub fn with_nominal(mut self, nominal: Real) -> MakeVanillaSwap {
        self.nominal = nominal;
        self
    }

    /// Sets the fixed-leg tenor, overriding the currency default
    /// (`makevanillaswap.cpp:263`).
    pub fn with_fixed_leg_tenor(mut self, tenor: Period) -> MakeVanillaSwap {
        self.fixed_tenor = Some(tenor);
        self
    }

    /// Sets the fixed-leg day count, overriding the currency default
    /// (`makevanillaswap.cpp:307`).
    pub fn with_fixed_leg_day_count(mut self, day_count: DayCounter) -> MakeVanillaSwap {
        self.fixed_day_count = Some(day_count);
        self
    }

    /// Sets the fixed-leg schedule calendar (`makevanillaswap.cpp:268`).
    pub fn with_fixed_leg_calendar(mut self, calendar: Calendar) -> MakeVanillaSwap {
        self.fixed_calendar = calendar;
        self
    }

    /// Sets the fixed-leg business-day convention (`makevanillaswap.cpp:274`).
    pub fn with_fixed_leg_convention(mut self, bdc: BusinessDayConvention) -> MakeVanillaSwap {
        self.fixed_convention = bdc;
        self
    }

    /// Sets the fixed-leg termination-date convention
    /// (`makevanillaswap.cpp:280`).
    pub fn with_fixed_leg_termination_date_convention(
        mut self,
        bdc: BusinessDayConvention,
    ) -> MakeVanillaSwap {
        self.fixed_termination_date_convention = bdc;
        self
    }

    /// Sets the fixed-leg end-of-month flag (`makevanillaswap.cpp:291`).
    pub fn with_fixed_leg_end_of_month(mut self, flag: bool) -> MakeVanillaSwap {
        self.fixed_end_of_month = flag;
        self
    }

    /// Sets the floating-leg schedule calendar (`makevanillaswap.cpp:318`).
    pub fn with_floating_leg_calendar(mut self, calendar: Calendar) -> MakeVanillaSwap {
        self.float_calendar = calendar;
        self
    }

    /// Sets the floating-leg business-day convention (`makevanillaswap.cpp:324`).
    pub fn with_floating_leg_convention(mut self, bdc: BusinessDayConvention) -> MakeVanillaSwap {
        self.float_convention = bdc;
        self
    }

    /// Sets the floating-leg termination-date convention
    /// (`makevanillaswap.cpp:330`).
    pub fn with_floating_leg_termination_date_convention(
        mut self,
        bdc: BusinessDayConvention,
    ) -> MakeVanillaSwap {
        self.float_termination_date_convention = bdc;
        self
    }

    /// Sets the floating-leg end-of-month flag (`makevanillaswap.cpp:341`).
    pub fn with_floating_leg_end_of_month(mut self, flag: bool) -> MakeVanillaSwap {
        self.float_end_of_month = flag;
        self
    }

    /// Prices the swap on `discounting_term_structure` rather than the index's
    /// forwarding curve (`makevanillaswap.cpp:249`).
    pub fn with_discounting_term_structure(
        mut self,
        discounting_term_structure: Handle<dyn YieldTermStructure>,
    ) -> MakeVanillaSwap {
        self.discounting_curve = Some(discounting_term_structure);
        self
    }

    /// Requests indexed (`Some(true)`) or at-par (`Some(false)`) coupons, checked
    /// against [`Settings`] at build time (`makevanillaswap.cpp:374`). See the
    /// module docs for the D5 refusal semantics.
    pub fn with_indexed_coupons(mut self, use_indexed_coupons: Option<bool>) -> MakeVanillaSwap {
        self.use_indexed_coupons = use_indexed_coupons;
        self
    }

    /// Builds the priced swap (C++ `operator VanillaSwap()` /
    /// `operator shared_ptr<VanillaSwap>()`, `makevanillaswap.cpp:52/57`).
    ///
    /// Derives the start date (explicit effective date or the settlement-days
    /// spot derivation), the end date, the fixed tenor and day count (given or
    /// currency-default), the two schedules and the fixed rate (given or
    /// fair-rate-filled), then attaches a [`DiscountingSwapEngine`].
    ///
    /// # Errors
    ///
    /// Returns an error when both an effective date and settlement days are set,
    /// when the requested coupon mode conflicts with [`Settings`], when the start
    /// date must be derived but no evaluation date is set, when the currency has
    /// no fixed-leg default, and propagates the swap construction and (for a
    /// fair-rate fill) the pricing.
    pub fn build(self) -> QlResult<VanillaSwap> {
        if self.effective_date.is_some() && self.settlement_days.is_some() {
            crate::fail!(
                "cannot set both an explicit effective date and settlement days; use one or the other"
            );
        }
        if let Some(requested_indexed) = self.use_indexed_coupons {
            let effective_indexed = !self.settings.using_at_par_coupons();
            if requested_indexed != effective_indexed {
                crate::fail!(
                    "with_indexed_coupons({requested_indexed}) conflicts with Settings::using_at_par_coupons(): \
                     the per-swap override is deferred with VanillaSwap, so the coupon mode must match Settings"
                );
            }
        }

        let start_date = self.start_date()?;
        let end_date = self.end_date(start_date);

        let currency = self.ibor_index.currency().clone();
        let fixed_tenor = match self.fixed_tenor {
            Some(tenor) => tenor,
            None => default_fixed_tenor(&currency)?,
        };
        let fixed_day_count = match &self.fixed_day_count {
            Some(day_count) => day_count.clone(),
            None => default_fixed_day_count(&currency)?,
        };
        let float_tenor = self.ibor_index.tenor();
        let float_day_count = self.ibor_index.day_counter().clone();

        let fixed_schedule = Schedule::new(
            start_date,
            end_date,
            fixed_tenor,
            self.fixed_calendar.clone(),
            self.fixed_convention,
            self.fixed_termination_date_convention,
            DateGeneration::Backward,
            self.fixed_end_of_month,
            Date::null(),
            Date::null(),
        );
        let float_schedule = Schedule::new(
            start_date,
            end_date,
            float_tenor,
            self.float_calendar.clone(),
            self.float_convention,
            self.float_termination_date_convention,
            DateGeneration::Backward,
            self.float_end_of_month,
            Date::null(),
            Date::null(),
        );

        let used_fixed_rate = match self.fixed_rate {
            Some(fixed_rate) => fixed_rate,
            None => {
                let mut temp = self.assemble(
                    0.0,
                    fixed_schedule.clone(),
                    float_schedule.clone(),
                    fixed_day_count.clone(),
                    float_day_count.clone(),
                )?;
                temp.fixed_vs_floating_mut().fair_rate()?
            }
        };

        self.assemble(
            used_fixed_rate,
            fixed_schedule,
            float_schedule,
            fixed_day_count,
            float_day_count,
        )
    }

    /// Derives the start date: an explicit effective date, or the spot date from
    /// the index (default) or an explicit settlement-day count, shifted by the
    /// forward start (`makevanillaswap.cpp:63-92`).
    fn start_date(&self) -> QlResult<Date> {
        if let Some(effective_date) = self.effective_date {
            return Ok(effective_date);
        }
        let ref_date = match self.settings.evaluation_date() {
            Some(today) => today,
            None => crate::fail!(
                "no evaluation date set: MakeVanillaSwap needs a reference date to derive the start date"
            ),
        };
        let spot_date = match self.settlement_days {
            None => {
                let adjusted = self
                    .ibor_index
                    .fixing_calendar()
                    .adjust(ref_date, BusinessDayConvention::Following);
                self.ibor_index.value_date(adjusted)?
            }
            Some(settlement_days) => {
                let adjusted = self
                    .float_calendar
                    .adjust(ref_date, BusinessDayConvention::Following);
                self.float_calendar.advance(
                    adjusted,
                    settlement_days as Integer,
                    TimeUnit::Days,
                    BusinessDayConvention::Following,
                    false,
                )
            }
        };
        let start = spot_date + self.forward_start;
        Ok(if self.forward_start.length() < 0 {
            self.float_calendar
                .adjust(start, BusinessDayConvention::Preceding)
        } else if self.forward_start.length() > 0 {
            self.float_calendar
                .adjust(start, BusinessDayConvention::Following)
        } else {
            start
        })
    }

    /// Derives the end date: an explicit termination date, or the start date plus
    /// the swap tenor with the optional maturity end-of-month roll
    /// (`makevanillaswap.cpp:94-102`).
    fn end_date(&self, start_date: Date) -> Date {
        if let Some(termination_date) = self.termination_date {
            return termination_date;
        }
        let mut end_date = start_date + self.swap_tenor;
        if self.float_end_of_month
            && allows_end_of_month(self.swap_tenor)
            && self.float_calendar.is_end_of_month(start_date)
        {
            end_date = self.float_calendar.end_of_month(end_date);
        }
        end_date
    }

    /// Assembles a [`VanillaSwap`] over the two schedules at `fixed_rate` and
    /// attaches the discounting engine (`makevanillaswap.cpp:187-199`).
    fn assemble(
        &self,
        fixed_rate: Rate,
        fixed_schedule: Schedule,
        float_schedule: Schedule,
        fixed_day_count: DayCounter,
        float_day_count: DayCounter,
    ) -> QlResult<VanillaSwap> {
        let mut swap = VanillaSwap::new(
            SwapType::Payer,
            self.nominal,
            fixed_schedule,
            fixed_rate,
            fixed_day_count,
            float_schedule,
            Shared::clone(&self.ibor_index),
            0.0,
            float_day_count,
            None,
            Shared::clone(&self.settings),
        )?;

        let discount_curve = match &self.discounting_curve {
            Some(curve) => curve.clone(),
            None => self.ibor_index.forwarding_term_structure().clone(),
        };
        let engine = shared_mut(DiscountingSwapEngine::new(
            discount_curve,
            Some(false),
            None,
            None,
            Shared::clone(&self.settings),
        ));
        swap.base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);

        Ok(swap)
    }
}

/// The fixed-leg tenor default keyed on the index currency
/// (`makevanillaswap.cpp:117-131`). Only the port's `EUR`/`USD` branches are
/// expressible (both `1Y`); other currencies error as C++'s `QL_FAIL` does.
fn default_fixed_tenor(currency: &Currency) -> QlResult<Period> {
    if *currency == Currency::eur() || *currency == Currency::usd() {
        Ok(Period::new(1, TimeUnit::Years))
    } else {
        crate::fail!("unknown fixed leg default tenor for {}", currency.code());
    }
}

/// The fixed-leg day-count default keyed on the index currency
/// (`makevanillaswap.cpp:148-163`). Only the port's `USD` (`Actual360`) and
/// `EUR` (`Thirty360(BondBasis)`) branches are expressible; other currencies
/// error as C++'s `QL_FAIL` does.
fn default_fixed_day_count(currency: &Currency) -> QlResult<DayCounter> {
    if *currency == Currency::usd() {
        Ok(Actual360::new())
    } else if *currency == Currency::eur() {
        Ok(Thirty360::with_convention(Convention::BondBasis))
    } else {
        crate::fail!("unknown fixed leg day counter for {}", currency.code());
    }
}
