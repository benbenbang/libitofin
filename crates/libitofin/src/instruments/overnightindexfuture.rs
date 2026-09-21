//! Overnight futures with simple or compounded reference-period accrual.

use crate::cashflows::RateAveraging;
use crate::errors::{QlError, QlResult};
use crate::event::event_has_occurred;
use crate::handle::Handle;
use crate::indexes::iborindex::OvernightIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::interestrate::Compounding;
use crate::quotes::Quote;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention::{self, Following, Preceding};
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::time::timeunit::TimeUnit;
use crate::types::Real;

/// A futures price quoted as `100 * (1 - rate - convexity adjustment)`.
///
/// The reference period is fixed. Historical fixings and market inputs remain
/// live through the supplied index, its settings and the convexity handle.
pub struct OvernightIndexFuture {
    base: InstrumentBase,
    index: Shared<OvernightIndex>,
    value_date: Date,
    maturity_date: Date,
    convexity: Handle<dyn Quote>,
    averaging: RateAveraging,
}

impl OvernightIndexFuture {
    /// Creates an engine-free overnight future.
    ///
    /// # Errors
    /// Rejects null, unordered or unsupported reference dates.
    pub fn new(
        index: Shared<OvernightIndex>,
        value_date: Date,
        maturity_date: Date,
        convexity: Handle<dyn Quote>,
        averaging: RateAveraging,
    ) -> QlResult<Self> {
        crate::require!(
            value_date >= Date::min_date()
                && maturity_date <= Date::max_date()
                && value_date < maturity_date,
            "invalid overnight future reference dates"
        );
        let base = InstrumentBase::new();
        base.register_with(index.observable());
        convexity.register_observer(&base.observer());
        index
            .settings()
            .register_eval_date_observer(&base.observer());
        Ok(Self {
            base,
            index,
            value_date,
            maturity_date,
            convexity,
            averaging,
        })
    }

    /// Index supplying daily fixings and forecasts.
    pub fn overnight_index(&self) -> &Shared<OvernightIndex> {
        &self.index
    }

    /// First accrual date, which may be a holiday.
    pub fn value_date(&self) -> Date {
        self.value_date
    }

    /// Exclusive end of the reference period.
    pub fn maturity_date(&self) -> Date {
        self.maturity_date
    }

    /// Daily-rate aggregation convention.
    pub fn averaging_method(&self) -> RateAveraging {
        self.averaging
    }

    /// Current convexity adjustment, zero for an empty handle.
    pub fn convexity_adjustment(&self) -> QlResult<Real> {
        let value = if self.convexity.is_empty() {
            0.0
        } else {
            self.convexity.current_link()?.value()?
        };
        crate::require!(value.is_finite(), "convexity adjustment must be finite");
        Ok(value)
    }

    fn date_operation(&self, operation: impl FnOnce() -> Date) -> QlResult<Date> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
            .map_err(|_| QlError::new("overnight future date range overflow", file!(), line!()))
    }

    fn next_date(&self, date: Date) -> QlResult<Date> {
        self.date_operation(|| {
            self.index
                .fixing_calendar()
                .advance(date, 1, TimeUnit::Days, Following, false)
        })
    }

    fn adjust_date(&self, date: Date, convention: BusinessDayConvention) -> QlResult<Date> {
        self.date_operation(|| self.index.fixing_calendar().adjust(date, convention))
    }

    fn historical_rate(&self, date: Date) -> QlResult<Real> {
        self.index.past_fixing(date)?.ok_or_else(|| {
            QlError::new(
                format!("missing rate on {date} for index {}", self.index.name()),
                file!(),
                line!(),
            )
        })
    }

    fn averaged_rate(&self, today: Date) -> QlResult<Real> {
        let dc = self.index.day_counter();
        let mut start = self.value_date;
        let mut fixing = self.adjust_date(start, Preceding)?;
        let mut sum = 0.0;
        while start < self.maturity_date {
            let end = self.next_date(start)?;
            let historical = if fixing < today {
                Some(self.historical_rate(fixing)?)
            } else if fixing == today {
                self.index.past_fixing(fixing)?
            } else {
                None
            };
            let rate = match historical {
                Some(rate) => rate,
                None => self
                    .index
                    .forwarding_term_structure()
                    .current_link()?
                    .forward_rate_between(
                        fixing,
                        end,
                        dc.clone(),
                        Compounding::Simple,
                        Frequency::Annual,
                        false,
                    )?
                    .rate(),
            };
            sum += rate * dc.year_fraction(start, end.min(self.maturity_date));
            start = end;
            fixing = end;
        }
        Ok(sum / dc.year_fraction(self.value_date, self.maturity_date))
    }

    fn compounded_rate(&self, today: Date) -> QlResult<Real> {
        let dc = self.index.day_counter();
        let mut product = 1.0;
        let mut forward_start = self.value_date;
        if today > self.value_date {
            let today = self.adjust_date(today, Following)?;
            forward_start = today;
            let mut start = self.value_date;
            let mut fixing = self.adjust_date(start, Preceding)?;
            while start < today {
                let rate = self.historical_rate(fixing)?;
                let end = self.next_date(start)?;
                product *= 1.0 + rate * dc.year_fraction(start, end);
                start = end;
                fixing = end;
            }
            if today < self.maturity_date
                && let Some(rate) = self.index.past_fixing(today)?
            {
                let tomorrow = self.next_date(today)?;
                product *= 1.0 + rate * dc.year_fraction(today, tomorrow);
                forward_start = tomorrow;
            }
        }
        let curve = self.index.forwarding_term_structure().current_link()?;
        product *= curve.discount_date(forward_start, false)?
            / curve.discount_date(self.maturity_date, false)?;
        Ok((product - 1.0) / dc.year_fraction(self.value_date, self.maturity_date))
    }
}

impl Instrument for OvernightIndexFuture {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }
    fn is_expired(&self) -> QlResult<bool> {
        event_has_occurred(self.maturity_date, self.index.settings(), None, None)
    }
    fn perform_calculations(&mut self) -> QlResult<()> {
        let today = self
            .index
            .settings()
            .evaluation_date()
            .ok_or_else(|| QlError::new("evaluation date is not set", file!(), line!()))?;
        crate::require!(
            today >= Date::min_date() && today <= Date::max_date(),
            "invalid evaluation date"
        );
        let rate = match self.averaging {
            RateAveraging::Simple => self.averaged_rate(today),
            RateAveraging::Compound => self.compounded_rate(today),
        }?;
        let value = 100.0 * (1.0 - rate - self.convexity_adjustment()?);
        crate::require!(value.is_finite(), "overnight future price must be finite");
        self.base.store_results(&InstrumentResults {
            value: Some(value),
            ..InstrumentResults::default()
        });
        Ok(())
    }
}
