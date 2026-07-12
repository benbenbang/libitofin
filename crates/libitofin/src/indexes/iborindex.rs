//! The Inter-Bank-Offered-Rate index.
//!
//! Port of `ql/indexes/iborindex.{hpp,cpp}`. [`IborIndex`] is the concrete
//! [`InterestRateIndex`] behind the floating leg: an
//! [`InterestRateIndexBase`] plus a business-day convention, an end-of-month
//! flag, and a [`Handle`] to the forwarding [`YieldTermStructure`] it reads
//! fixings off.
//!
//! It supplies the two members the base leaves abstract:
//! [`maturity_date`](InterestRateIndex::maturity_date) advances the value date
//! by the tenor under the index convention, and
//! [`forecast_fixing`](InterestRateIndex::forecast_fixing) derives the simple
//! forward rate from the curve's discount factors between the value and
//! maturity dates. The index registers its forwarding observer with the curve
//! handle, so a relinked or changed curve notifies the index's observers.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::{InterestRateIndex, InterestRateIndexBase};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::types::{Natural, Rate, Time};
use crate::{currency::Currency, require};

/// A concrete Inter-Bank-Offered-Rate index (e.g. Libor, Euribor).
///
/// Wraps an [`InterestRateIndexBase`] with the forwarding curve and the
/// convention the maturity calculation needs. Built with a possibly empty curve
/// handle, exactly as the C++ default `Handle<YieldTermStructure> h = {}`
/// allows; a fixing forecast on an empty handle is an error, not a panic (D4).
pub struct IborIndex {
    base: InterestRateIndexBase,
    convention: BusinessDayConvention,
    end_of_month: bool,
    term_structure: Handle<dyn YieldTermStructure>,
}

impl IborIndex {
    /// Builds an index over `forwarding`, registering with the curve handle.
    ///
    /// Mirrors the C++ constructor: it composes the base (which normalizes the
    /// tenor and wires evaluation-date and fixing-history observation), stores
    /// the convention and end-of-month flag, and registers the index's
    /// forwarding observer with the curve handle so a relink notifies observers.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        family_name: String,
        tenor: Period,
        settlement_days: Natural,
        currency: Currency,
        fixing_calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
        day_counter: DayCounter,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        let base = InterestRateIndexBase::new(
            family_name,
            tenor,
            settlement_days,
            currency,
            fixing_calendar,
            day_counter,
            settings,
        );
        forwarding.register_observer(&base.observer());
        IborIndex {
            base,
            convention,
            end_of_month,
            term_structure: forwarding,
        }
    }

    /// The convention applied when rolling the value date to maturity.
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.convention
    }

    /// Whether the maturity roll keeps to month ends.
    pub fn end_of_month(&self) -> bool {
        self.end_of_month
    }

    /// The curve used to forecast fixings (`forwardingTermStructure`).
    pub fn forwarding_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.term_structure
    }

    /// The simple forward rate over `[d1, d2]` with year fraction `t`, read off
    /// the forwarding curve (the C++ private `forecastFixing(d1, d2, t)`).
    ///
    /// Kept private, as in C++, so a caller cannot pass mismatched dates and ask
    /// a 6-month index for a 1-year fixing.
    fn forecast_fixing_between(&self, d1: Date, d2: Date, t: Time) -> QlResult<Rate> {
        require!(
            !self.term_structure.is_empty(),
            "null term structure set to this instance of {}",
            self.name()
        );
        let curve = self.term_structure.current_link()?;
        let disc1 = curve.discount_date(d1, false)?;
        let disc2 = curve.discount_date(d2, false)?;
        Ok((disc1 / disc2 - 1.0) / t)
    }
}

impl InterestRateIndex for IborIndex {
    fn base(&self) -> &InterestRateIndexBase {
        &self.base
    }

    fn maturity_date(&self, value_date: Date) -> QlResult<Date> {
        Ok(self.fixing_calendar().advance_by_period(
            value_date,
            self.tenor(),
            self.convention,
            self.end_of_month,
        ))
    }

    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate> {
        let d1 = self.value_date(fixing_date)?;
        let d2 = self.maturity_date(d1)?;
        let t = self.day_counter().year_fraction(d1, d2);
        let positive_time = t > 0.0;
        require!(
            positive_time,
            "cannot calculate forward rate between {d1:?} and {d2:?}: non positive time ({t}) using {} daycounter",
            self.day_counter().name()
        );
        self.forecast_fixing_between(d1, d2, t)
    }
}
