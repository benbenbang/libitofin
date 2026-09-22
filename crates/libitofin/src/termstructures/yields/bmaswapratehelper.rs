//! BMA municipal swap bootstrap helper, matching QuantLib's `ratehelpers.cpp`.
use crate::errors::{QlError, QlResult};
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::{BMAIndex, IborIndex, Index, InterestRateIndex};
use crate::instrument::Instrument;
use crate::instruments::{BMASwap, SwapType};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::swap::DiscountingSwapEngine;
use crate::quotes::Quote;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::bootstraphelper::{
    BootstrapHelperBase, RateHelper, RelativeDateRateHelper,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::calendars::jointcalendar::{JointCalendar, JointCalendarRule};
use crate::time::{
    businessdayconvention::BusinessDayConvention as Bdc, calendar::Calendar, date::Date,
    daycounter::DayCounter, period::Period, schedule::MakeSchedule, timeunit::TimeUnit,
};
use std::{
    cell::RefCell,
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Weak,
};

/// Fits the BMA forecast curve from fractions of a supplied Ibor forecast curve.
pub struct BMASwapRateHelper {
    base: BootstrapHelperBase,
    swap: RefCell<Option<BMASwap>>,
    date_error: RefCell<Option<QlError>>,
    tenor: Period,
    settlement_days: u32,
    calendar: Calendar,
    bma_period: Period,
    bma_convention: Bdc,
    bma_day_count: DayCounter,
    bma_index: Shared<BMAIndex>,
    ibor_index: Shared<IborIndex>,
    settings: Shared<Settings<Date>>,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
}
impl BMASwapRateHelper {
    /// Constructs a relative-date helper retaining live quotes and index histories.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        fraction: Handle<dyn Quote>,
        tenor: Period,
        settlement_days: u32,
        calendar: Calendar,
        bma_period: Period,
        bma_convention: Bdc,
        bma_day_count: DayCounter,
        bma_index: &Shared<BMAIndex>,
        ibor_index: &Shared<IborIndex>,
    ) -> QlResult<Shared<Self>> {
        crate::require!(!fraction.is_empty(), "BMA fraction quote is empty");
        crate::require!(
            Shared::ptr_eq(bma_index.base().settings(), ibor_index.base().settings()),
            "BMA and Ibor indexes must share settings"
        );
        crate::require!(
            settlement_days <= (Date::max_date() - Date::min_date()) as u32,
            "BMA settlement days outside supported range"
        );
        let span = i64::from(Date::max_date() - Date::min_date());
        for p in [tenor, bma_period, ibor_index.tenor()] {
            let days = match p.units() {
                TimeUnit::Days => 1,
                TimeUnit::Weeks => 7,
                TimeUnit::Months => 28,
                TimeUnit::Years => 365,
                _ => crate::fail!("BMA periods must use calendar units"),
            };
            crate::require!(
                p.length() > 0 && i64::from(p.length()) * days <= span,
                "BMA periods must be positive and within supported range"
            );
        }
        let settings = bma_index.base().settings().clone();
        let helper = Shared::new_cyclic(|weak: &Weak<Self>| {
            let weak = weak.clone();
            let base = BootstrapHelperBase::new_relative(
                fraction,
                settings.clone(),
                true,
                Box::new(move || {
                    if let Some(helper) = weak.upgrade() {
                        helper.initialize_dates();
                    }
                }),
            );
            bma_index.observable().register_observer(&base.observer());
            ibor_index.observable().register_observer(&base.observer());
            Self {
                base,
                swap: RefCell::new(None),
                date_error: RefCell::new(None),
                tenor,
                settlement_days,
                calendar,
                bma_period,
                bma_convention,
                bma_day_count,
                bma_index: bma_index.clone(),
                ibor_index: ibor_index.clone(),
                settings,
                term_structure_handle: RelinkableHandle::empty(),
            }
        });
        helper.try_initialize_dates()?;
        Ok(helper)
    }
    /// Atomically rebuilds dates and stores errors until a subsequent successful update.
    pub fn try_initialize_dates(&self) -> QlResult<()> {
        let result = catch_unwind(AssertUnwindSafe(|| self.rebuild_dates())).unwrap_or_else(|_| {
            Err(QlError::new(
                "invalid BMA schedule or date range",
                file!(),
                line!(),
            ))
        });
        match result {
            Ok((swap, earliest, maturity, latest)) => {
                *self.swap.borrow_mut() = Some(swap);
                self.base.set_earliest_date(earliest);
                self.base.set_maturity_date(maturity);
                self.base.set_latest_relevant_date(latest);
                self.base.set_pillar_date(latest);
                self.base.set_latest_date(latest);
                *self.date_error.borrow_mut() = None;
                Ok(())
            }
            Err(error) => {
                *self.date_error.borrow_mut() = Some(error.clone());
                Err(error)
            }
        }
    }
    fn rebuild_dates(&self) -> QlResult<(BMASwap, Date, Date, Date)> {
        let today = self
            .base
            .evaluation_date()
            .ok_or_else(|| QlError::new("evaluation date is not set", file!(), line!()))?;
        crate::require!(
            today >= Date::min_date() + 14 && today <= Date::max_date() - 14,
            "BMA evaluation date outside supported range"
        );
        let jc = JointCalendar::new(
            vec![self.calendar.clone(), self.ibor_index.fixing_calendar()],
            JointCalendarRule::JoinHolidays,
        );
        let earliest = self.calendar.advance(
            jc.adjust(today, Bdc::Following),
            self.settlement_days as i32,
            TimeUnit::Days,
            Bdc::Following,
            false,
        );
        let maturity = earliest + self.tenor;
        let bma = shared(
            self.bma_index
                .clone_with(self.term_structure_handle.handle()),
        );
        let bs = MakeSchedule::new()
            .from(earliest)
            .to(maturity)
            .with_tenor(self.bma_period)
            .with_calendar(bma.fixing_calendar())
            .with_convention(self.bma_convention)
            .backwards()
            .build();
        let ls = MakeSchedule::new()
            .from(earliest)
            .to(maturity)
            .with_tenor(self.ibor_index.tenor())
            .with_calendar(self.ibor_index.fixing_calendar())
            .with_convention(self.ibor_index.business_day_convention())
            .end_of_month(self.ibor_index.end_of_month())
            .backwards()
            .build();
        let mut swap = BMASwap::new(
            SwapType::Payer,
            100.0,
            ls,
            0.75,
            0.0,
            self.ibor_index.clone(),
            self.ibor_index.day_counter().clone(),
            bs,
            bma.clone(),
            self.bma_day_count.clone(),
            self.settings.clone(),
        )?;
        swap.base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                self.ibor_index.forwarding_term_structure().clone(),
                None,
                None,
                None,
                self.settings.clone(),
            )) as SharedMut<dyn PricingEngine>);
        let maturity = swap.swap().maturity_date()?;
        let d = self.calendar.adjust(maturity, Bdc::Following);
        let next = crate::indexes::bmaindex::previous_wednesday(d + 7);
        let latest = bma.value_date(bma.fixing_calendar().adjust(next, Bdc::Following))?;
        Ok((swap, earliest, maturity, latest))
    }
}
impl AsObservable for BMASwapRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl RateHelper for BMASwapRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }
    fn implied_quote(&self) -> QlResult<f64> {
        if let Some(error) = self.date_error.borrow().as_ref() {
            return Err(error.clone());
        }
        self.base.term_structure()?;
        let mut slot = self.swap.borrow_mut();
        let swap = slot.as_mut().expect("validated BMA helper has a swap");
        swap.swap_mut().deep_update();
        swap.fair_libor_fraction()
    }
    fn set_term_structure(&self, curve: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(curve));
        self.base.set_term_structure(curve);
    }
}
impl RelativeDateRateHelper for BMASwapRateHelper {
    fn initialize_dates(&self) {
        let _ = self.try_initialize_dates();
    }
}

#[cfg(test)]
#[path = "bmaswapratehelper_tests.rs"]
mod tests;
