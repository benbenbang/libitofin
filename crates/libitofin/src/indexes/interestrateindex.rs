//! The interest-rate index base.
//!
//! Port of `ql/indexes/interestrateindex.{hpp,cpp}`. `InterestRateIndex`
//! refines [`Index`] with a family name, tenor, fixing lag, currency and day
//! counter, the fixing/value-date algebra, and the fixing decision tree where a
//! historical fixing (D11 store) and a forecast curve meet. It stays abstract:
//! [`maturity_date`](InterestRateIndex::maturity_date) and
//! [`forecast_fixing`](InterestRateIndex::forecast_fixing) are the pure-virtual
//! members a concrete index (an `IborIndex`) supplies.
//!
//! Shared state lives in [`InterestRateIndexBase`], the analogue of
//! `CouponBase`: a concrete index embeds one, hands it back through
//! [`base`](InterestRateIndex::base), and the blanket
//! `impl<T: InterestRateIndex> Index for T` answers the whole [`Index`] surface
//! from it - so [`fixing`](Index::fixing), in particular, is written once and
//! cannot be re-derived wrongly per index (the lesson of the `Coupon`/`CashFlow`
//! blanket).

use crate::currency::Currency;
use crate::errors::QlResult;
use crate::indexes::index::Index;
use crate::patterns::observable::{Observable, Observer, ResetThenNotify};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate};
use crate::{fail, require};

/// Shared state of every interest-rate index (`InterestRateIndex`'s members).
///
/// Built by [`new`](InterestRateIndexBase::new), which normalizes the tenor and
/// composes the index name exactly as the C++ constructor does, then registers
/// the index's forwarding observer with both the evaluation date and its own
/// fixing history (the two `registerWith` calls). Downstream observers register
/// with [`observable`](InterestRateIndexBase::observable); a change to either
/// source is re-broadcast through it, the port of `Index::update`.
pub struct InterestRateIndexBase {
    family_name: String,
    tenor: Period,
    fixing_days: Natural,
    currency: Currency,
    day_counter: DayCounter,
    fixing_calendar: Calendar,
    name: String,
    settings: Shared<Settings<Date>>,
    observable: Shared<Observable>,
    #[allow(dead_code)]
    forwarder: SharedMut<ResetThenNotify>,
}

impl InterestRateIndexBase {
    /// Builds the shared state, wiring the index's observation of the
    /// evaluation date and its fixing history.
    ///
    /// The tenor is normalized as in the C++ constructor (a whole number of
    /// months becomes years, days left alone) and the name composed the same
    /// way (`ON`/`TN`/`SN` for a one-day tenor at 0/1/2 fixing days, otherwise
    /// the short period, then the day-counter name).
    pub fn new(
        family_name: String,
        tenor: Period,
        fixing_days: Natural,
        currency: Currency,
        fixing_calendar: Calendar,
        day_counter: DayCounter,
        settings: Shared<Settings<Date>>,
    ) -> Self {
        let tenor = normalize_tenor(tenor);
        let name = compose_name(&family_name, tenor, fixing_days, &day_counter);

        let (observable, forwarder) = ResetThenNotify::forwarder();
        let observer = forwarder.clone() as SharedMut<dyn Observer>;
        settings.register_eval_date_observer(&observer);
        settings.register_fixing_observer(&name, &observer);

        InterestRateIndexBase {
            family_name,
            tenor,
            fixing_days,
            currency,
            day_counter,
            fixing_calendar,
            name,
            settings,
            observable,
            forwarder,
        }
    }

    /// The observable the index broadcasts its changes through.
    pub fn observable(&self) -> &Observable {
        &self.observable
    }
}

/// A whole number of months becomes years, mirroring the C++ constructor's
/// deliberately partial normalization (days are left untouched).
fn normalize_tenor(tenor: Period) -> Period {
    if tenor.units() == TimeUnit::Months && tenor.length() % 12 == 0 {
        Period::new(tenor.length() / 12, TimeUnit::Years)
    } else {
        tenor
    }
}

/// Composes the index name as `InterestRateIndex`'s constructor does.
fn compose_name(
    family_name: &str,
    tenor: Period,
    fixing_days: Natural,
    day_counter: &DayCounter,
) -> String {
    let period = if tenor == Period::new(1, TimeUnit::Days) {
        match fixing_days {
            0 => "ON".to_string(),
            1 => "TN".to_string(),
            2 => "SN".to_string(),
            _ => format!("{tenor}"),
        }
    } else {
        format!("{tenor}")
    };
    format!("{family_name}{period} {}", day_counter.name())
}

/// The interest-rate index interface (`InterestRateIndex`).
///
/// A concrete index supplies [`base`](InterestRateIndex::base) and the two
/// abstract calculations, [`maturity_date`](InterestRateIndex::maturity_date)
/// and [`forecast_fixing`](InterestRateIndex::forecast_fixing); the inspectors
/// and the [`fixing_date`](InterestRateIndex::fixing_date) /
/// [`value_date`](InterestRateIndex::value_date) algebra are provided, and can
/// be overridden by conventions that need it (the C++ `virtual` on those two).
pub trait InterestRateIndex {
    /// The embedded shared state.
    fn base(&self) -> &InterestRateIndexBase;

    /// The maturity date of the loan fixed on `value_date` (pure virtual in
    /// C++: the concrete index applies its tenor and convention).
    fn maturity_date(&self, value_date: Date) -> QlResult<Date>;

    /// The forecast fixing at `fixing_date` from the index's forwarding curve
    /// (pure virtual in C++).
    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<Rate>;

    /// The family name (e.g. `Euribor`).
    fn family_name(&self) -> &str {
        &self.base().family_name
    }

    /// The tenor (normalized at construction).
    fn tenor(&self) -> Period {
        self.base().tenor
    }

    /// The number of fixing (settlement) days.
    fn fixing_days(&self) -> Natural {
        self.base().fixing_days
    }

    /// The index currency.
    fn currency(&self) -> &Currency {
        &self.base().currency
    }

    /// The index day counter.
    fn day_counter(&self) -> &DayCounter {
        &self.base().day_counter
    }

    /// The fixing date for a given `value_date`: `value_date` moved back
    /// `fixing_days` business days on the fixing calendar.
    fn fixing_date(&self, value_date: Date) -> Date {
        let base = self.base();
        base.fixing_calendar.advance(
            value_date,
            -(base.fixing_days as Integer),
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )
    }

    /// The value date for a given `fixing_date`: `fixing_date` moved forward
    /// `fixing_days` business days. Requires a valid fixing date, as in C++.
    fn value_date(&self, fixing_date: Date) -> QlResult<Date> {
        let base = self.base();
        require!(
            base.fixing_calendar.is_business_day(fixing_date),
            "{fixing_date:?} is not a valid fixing date"
        );
        Ok(base.fixing_calendar.advance(
            fixing_date,
            base.fixing_days as Integer,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        ))
    }
}

/// The whole [`Index`] surface, answered from an interest-rate index's base -
/// the port of `InterestRateIndex`'s `Index`-interface overrides, including the
/// `interestrateindex.cpp:63` fixing decision tree.
impl<T: InterestRateIndex> Index for T {
    fn name(&self) -> String {
        self.base().name.clone()
    }

    fn fixing_calendar(&self) -> Calendar {
        self.base().fixing_calendar.clone()
    }

    fn is_valid_fixing_date(&self, fixing_date: Date) -> bool {
        self.base().fixing_calendar.is_business_day(fixing_date)
    }

    fn settings(&self) -> &Settings<Date> {
        &self.base().settings
    }

    fn observable(&self) -> &Observable {
        &self.base().observable
    }

    fn fixing(&self, fixing_date: Date, forecast_todays_fixing: bool) -> QlResult<Rate> {
        require!(
            self.is_valid_fixing_date(fixing_date),
            "Fixing date {fixing_date:?} is not valid"
        );

        let today = match self.settings().evaluation_date() {
            Some(today) => today,
            None => fail!("no evaluation date set: an index fixing needs a reference date"),
        };

        if fixing_date > today || (fixing_date == today && forecast_todays_fixing) {
            return self.forecast_fixing(fixing_date);
        }

        if fixing_date < today || self.settings().enforces_todays_historic_fixings() {
            return match self.settings().fixing(&self.name(), fixing_date) {
                Some(rate) => Ok(rate),
                None => fail!("Missing {} fixing for {fixing_date:?}", self.name()),
            };
        }

        if let Some(rate) = self.settings().fixing(&self.name(), fixing_date) {
            return Ok(rate);
        }
        self.forecast_fixing(fixing_date)
    }
}
