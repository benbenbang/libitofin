//! SIFMA municipal index from QuantLib's `bmaindex.cpp`.
use crate::currency::Currency;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::{Index, InterestRateIndex, InterestRateIndexBase};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::unitedstates::{Market, UnitedStates};
use crate::time::daycounters::actualactual::{ActualActual, Convention};
use crate::time::{
    businessdayconvention::BusinessDayConvention as Bdc,
    date::Date,
    period::Period,
    schedule::{MakeSchedule, Schedule},
    timeunit::TimeUnit,
};

/// Weekly municipal index, fixing on Wednesday or its next business day.
pub struct BMAIndex {
    base: InterestRateIndexBase,
    forwarding: Handle<dyn YieldTermStructure>,
}
impl BMAIndex {
    /// Creates an index retaining its forecasting handle and fixing history.
    pub fn new(
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> Self {
        let base = InterestRateIndexBase::new(
            "BMA".into(),
            Period::new(1, TimeUnit::Weeks),
            1,
            Currency::usd(),
            UnitedStates::new(Market::GovernmentBond),
            ActualActual::with_convention(Convention::ISDA),
            settings,
        );
        forwarding.register_observer(&base.observer());
        Self { base, forwarding }
    }
    /// Rebuilds against another curve while sharing settings and history.
    pub fn clone_with(&self, forwarding: Handle<dyn YieldTermStructure>) -> Self {
        Self::new(forwarding, self.base.settings().clone())
    }
    /// Forecast curve handle.
    pub fn forwarding_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.forwarding
    }
    /// Weekly fixing schedule bracketing both dates.
    pub fn fixing_schedule(&self, start: Date, end: Date) -> QlResult<Schedule> {
        crate::require!(start <= end, "fixing schedule start is after end");
        crate::require!(
            start >= Date::min_date() + 7 && end <= Date::max_date() - 7,
            "fixing schedule outside supported dates"
        );
        Ok(MakeSchedule::new()
            .from(previous_wednesday(start))
            .to(previous_wednesday(end + 7))
            .with_tenor(Period::new(1, TimeUnit::Weeks))
            .with_calendar(self.fixing_calendar())
            .with_convention(Bdc::Following)
            .forwards()
            .build())
    }
}
pub(crate) fn previous_wednesday(date: Date) -> Date {
    date - ((date.weekday() as i32 + 3) % 7)
}
impl InterestRateIndex for BMAIndex {
    fn base(&self) -> &InterestRateIndexBase {
        &self.base
    }
    fn valid_fixing_date(&self, date: Date) -> bool {
        if date < Date::min_date() + 7 || date > Date::max_date() {
            return false;
        }
        let cal = self.fixing_calendar();
        let mut d = previous_wednesday(date);
        while d < date {
            if cal.is_business_day(d) {
                return false;
            }
            d += 1;
        }
        cal.is_business_day(date)
    }
    fn maturity_date(&self, value_date: Date) -> QlResult<Date> {
        crate::require!(
            value_date >= Date::min_date() + 7 && value_date <= Date::max_date() - 14,
            "BMA value date outside supported range"
        );
        let cal = self.fixing_calendar();
        let fixing = cal.advance(value_date, -1, TimeUnit::Days, Bdc::Following, false);
        Ok(cal.advance(
            previous_wednesday(fixing + 7),
            1,
            TimeUnit::Days,
            Bdc::Following,
            false,
        ))
    }
    fn forecast_fixing(&self, fixing_date: Date) -> QlResult<f64> {
        crate::require!(
            !self.forwarding.is_empty(),
            "BMA forecasting curve is empty"
        );
        let start = self.value_date(fixing_date)?;
        let end = self.maturity_date(start)?;
        let dt = self.day_counter().year_fraction(start, end);
        let curve = self.forwarding.current_link()?;
        Ok((curve.discount_date(start, false)? / curve.discount_date(end, false)? - 1.0) / dt)
    }
}
