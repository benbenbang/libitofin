//! CPI-indexed coupon bond with explicit base CPI and indexed principal.
//!
//! Adapted from QuantLib `ql/instruments/bonds/cpibond.{hpp,cpp}` at revision
//! `9863b578af0caa4cecabf697196533e84a8308b6`.
//! Copyright (C) 2010, 2011 Chris Kenyon. Distributed under the QuantLib license.

use crate::cashflows::CpiLeg;
use crate::errors::QlResult;
use crate::indexes::Index;
use crate::indexes::inflationindex::{CpiInterpolationType, ZeroInflationIndex};
use crate::instrument::Instrument;
use crate::instruments::Bond;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::{Natural, Rate, Real};

/// Plain CPI-scaled coupons and a full CPI-scaled terminal principal.
///
/// Prices retain the ordinary [`Bond`] convention: per 100 of the original
/// nominal, not per 100 of the inflation-adjusted redemption. The deprecated
/// growth-only constructor is intentionally absent.
/// Generic coupon-BPS/ATM formulas on the base do not represent sensitivity to
/// the contractual raw CPI coupon rate because their weights omit the CPI
/// ratio. Language facades reject those four coupon-analytics methods for CPI
/// bonds; flat-yield risk and ordinary valuation remain available.
pub struct CpiBond {
    bond: Bond,
    frequency: Frequency,
    day_counter: DayCounter,
    base_cpi: Real,
    observation_lag: Period,
    index: Shared<ZeroInflationIndex>,
    interpolation: CpiInterpolationType,
}
impl CpiBond {
    /// Constructs a constant-face CPI bond using the modern explicit-base path.
    ///
    /// The payment calendar defaults to the schedule calendar. Coupon rates
    /// repeat their final value over remaining periods. The indexed principal
    /// is observed at schedule maturity, not the adjusted payment date.
    ///
    /// # Errors
    ///
    /// Rejects invalid numeric inputs, dates, settlement lag or mixed settings.
    /// Propagates CPI-leg construction and base-bond errors. Construction does
    /// not require future or historical CPI observations to be available.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        face_amount: Real,
        base_cpi: Real,
        observation_lag: Period,
        index: Shared<ZeroInflationIndex>,
        interpolation: CpiInterpolationType,
        schedule: Schedule,
        coupons: Vec<Rate>,
        accrual_day_counter: DayCounter,
        payment_convention: BusinessDayConvention,
        issue_date: Option<Date>,
        payment_calendar: Option<Calendar>,
        ex_coupon_period: Option<Period>,
        ex_coupon_calendar: Calendar,
        ex_coupon_convention: BusinessDayConvention,
        ex_coupon_end_of_month: bool,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        require!(schedule.len() >= 2, "schedule spans no period");
        require!(
            settlement_days <= (Date::max_date() - Date::min_date()) as u32,
            "settlement days exceed supported date range"
        );
        require!(
            face_amount.is_finite() && face_amount > 0.0 && base_cpi.is_finite() && base_cpi > 0.0,
            "positive finite face amount and base CPI required"
        );
        require!(
            !coupons.is_empty() && coupons.iter().all(|r| r.is_finite()),
            "nonempty finite coupon rates required"
        );
        require!(
            std::ptr::eq(&*settings, Index::settings(&*index)),
            "bond and index must share settings"
        );
        require!(
            issue_date.is_none_or(|d| d != Date::null())
                && schedule.dates().iter().all(|d| *d != Date::null())
                && schedule.dates().windows(2).all(|d| d[0] < d[1]),
            "non-null increasing schedule and issue dates required"
        );
        require!(
            ex_coupon_period.is_none_or(|p| p.length() >= 0),
            "ex-coupon period must be nonnegative"
        );
        let maturity = schedule.end_date();
        let frequency = if schedule.has_tenor() {
            schedule.tenor().frequency()
        } else {
            Frequency::NoFrequency
        };
        let calendar = payment_calendar.unwrap_or_else(|| schedule.calendar().clone());
        let mut builder = CpiLeg::new(schedule, index.clone(), base_cpi, observation_lag)
            .with_notional(face_amount)
            .with_fixed_rates(coupons)
            .with_payment_day_counter(accrual_day_counter.clone())
            .with_payment_adjustment(payment_convention)
            .with_payment_calendar(calendar.clone())
            .with_observation_interpolation(interpolation)
            .with_subtract_inflation_nominal(false);
        if let Some(period) = ex_coupon_period.filter(|p| p.length() != 0) {
            builder = builder.with_ex_coupon_period(
                period,
                ex_coupon_calendar,
                ex_coupon_convention,
                ex_coupon_end_of_month,
            );
        }
        let mut flows = builder.build()?;
        let redemption = flows
            .pop()
            .ok_or_else(|| crate::errors::QlError::new("empty CPI leg", file!(), line!()))?;
        let mut bond = Bond::new(settlement_days, calendar, issue_date, flows, settings)?;
        bond.set_indexed_redemption(face_amount, redemption)?;
        bond.set_maturity_date(maturity);
        bond.base().register_with(Index::observable(&*index));
        Ok(Self {
            bond,
            frequency,
            day_counter: accrual_day_counter,
            base_cpi,
            observation_lag,
            index,
            interpolation,
        })
    }

    /// Base instrument retaining all indexed cash flows and pricing state.
    pub fn bond(&self) -> &Bond {
        &self.bond
    }
    /// Mutable base instrument for attaching engines and requesting prices.
    pub fn bond_mut(&mut self) -> &mut Bond {
        &mut self.bond
    }
    /// Transfers the base while retaining the full index-dependent cash-flow graph.
    pub fn into_bond(self) -> Bond {
        self.bond
    }
    /// Schedule tenor frequency, or `NoFrequency` for an explicit-date schedule.
    pub fn frequency(&self) -> Frequency {
        self.frequency
    }
    /// Coupon accrual day counter.
    pub fn day_counter(&self) -> &DayCounter {
        &self.day_counter
    }
    /// Contractually fixed reference CPI.
    pub fn base_cpi(&self) -> Real {
        self.base_cpi
    }
    /// Observation lag shared by coupons and principal.
    pub fn observation_lag(&self) -> Period {
        self.observation_lag
    }
    /// Retained live inflation index.
    pub fn cpi_index(&self) -> &Shared<ZeroInflationIndex> {
        &self.index
    }
    /// Flat or linear CPI observation convention.
    pub fn observation_interpolation(&self) -> CpiInterpolationType {
        self.interpolation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::Handle;
    use crate::indexes::inflation::UkRpi;
    use crate::interestrate::Compounding;
    use crate::pricingengines::bond::DiscountingBondEngine;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::time::calendars::NullCalendar;
    use crate::time::date::Month::*;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::timeunit::TimeUnit;

    fn fixture(interpolation: CpiInterpolationType) -> (CpiBond, Shared<ZeroInflationIndex>) {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(15, June, 2025));
        let index = shared(UkRpi::new(settings.clone()));
        for (month, value) in [(March, 104.0), (April, 105.0), (May, 107.0)] {
            index.add_fixing(Date::new(1, month, 2025), value).unwrap();
        }
        let mut bond = CpiBond::new(
            0,
            1000.0,
            100.0,
            Period::new(3, TimeUnit::Months),
            index.clone(),
            interpolation,
            Schedule::from_dates(vec![
                Date::new(15, January, 2025),
                Date::new(15, July, 2025),
            ]),
            vec![0.02],
            Actual365Fixed::new(),
            BusinessDayConvention::Unadjusted,
            Some(Date::new(15, January, 2025)),
            None,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            settings.clone(),
        )
        .unwrap();
        let curve = shared(FlatForward::with_rate(
            Date::new(15, June, 2025),
            0.0,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        bond.bond_mut()
            .base_mut()
            .set_pricing_engine(shared_mut(DiscountingBondEngine::new(
                Handle::new(curve),
                None,
                settings,
            )));
        (bond, index)
    }
    #[test]
    fn indexed_principal_and_original_face_price_units() {
        let (mut bond, _) = fixture(CpiInterpolationType::Flat);
        let coupon = 1000.0 * 0.02 * 181.0 / 365.0 * 1.05;
        let expected = 1050.0 + coupon;
        assert_eq!(bond.bond().cashflows().len(), 2);
        assert_eq!(bond.bond().redemptions().len(), 1);
        assert!((bond.bond().redemptions()[0].amount().unwrap() - 1050.0).abs() < 1e-12);
        assert!((bond.bond_mut().npv().unwrap() - expected).abs() < 1e-10);
        assert!((bond.bond_mut().dirty_price().unwrap() - expected / 10.0).abs() < 1e-10);
        assert_eq!(bond.bond().notional(None).unwrap(), 1000.0);
        assert_eq!(
            bond.bond()
                .notional(Some(Date::new(15, July, 2025)))
                .unwrap(),
            0.0
        );
        assert!(
            (bond
                .bond()
                .accrued_amount(Some(Date::new(17, June, 2025)))
                .unwrap()
                - 100.0 * 0.02 * 153.0 / 365.0 * 1.04)
                .abs()
                < 1e-12
        );
    }
    #[test]
    fn linear_observation_and_live_fixing_invalidation() {
        let (mut bond, index) = fixture(CpiInterpolationType::Linear);
        let ratio = (105.0 + 2.0 * 14.0 / 31.0) / 100.0;
        assert!((bond.bond().redemptions()[0].amount().unwrap() - 1000.0 * ratio).abs() < 1e-10);
        let before = bond.bond_mut().npv().unwrap();
        index.clear_fixings();
        assert!(bond.bond_mut().npv().is_err());
        for (month, value) in [(March, 105.0), (April, 106.0), (May, 108.0)] {
            index.add_fixing(Date::new(1, month, 2025), value).unwrap();
        }
        assert!(bond.bond_mut().npv().unwrap() > before);
        let base = bond.into_bond();
        drop(index);
        assert!(base.redemptions()[0].amount().unwrap().is_finite());
        let snapshot = base.unpriced_snapshot();
        assert!(
            (snapshot.redemptions()[0].amount().unwrap() - base.redemptions()[0].amount().unwrap())
                .abs()
                < 1e-12
        );
    }
    #[test]
    fn coupon_vectors_repeat_last_and_support_distinct_period_rates() {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(15, June, 2026));
        let index = shared(UkRpi::new(settings.clone()));
        index.add_fixing(Date::new(1, April, 2025), 105.0).unwrap();
        index
            .add_fixing(Date::new(1, October, 2025), 110.0)
            .unwrap();
        let build = |rates| {
            CpiBond::new(
                0,
                1000.0,
                100.0,
                Period::new(3, TimeUnit::Months),
                index.clone(),
                CpiInterpolationType::Flat,
                Schedule::from_dates(vec![
                    Date::new(15, January, 2025),
                    Date::new(15, July, 2025),
                    Date::new(15, January, 2026),
                ]),
                rates,
                Actual365Fixed::new(),
                BusinessDayConvention::Unadjusted,
                None,
                None,
                None,
                NullCalendar::new(),
                BusinessDayConvention::Unadjusted,
                false,
                settings.clone(),
            )
            .unwrap()
        };
        let repeated = build(vec![0.02]);
        let explicit = build(vec![0.02, 0.02]);
        for (left, right) in repeated
            .bond()
            .cashflows()
            .iter()
            .zip(explicit.bond().cashflows())
        {
            assert!((left.amount().unwrap() - right.amount().unwrap()).abs() < 1e-12);
        }
        let varying = build(vec![0.02, 0.03]);
        let first = varying.bond().cashflows()[0].amount().unwrap();
        let second = varying.bond().cashflows()[1].amount().unwrap();
        assert!((first - 1000.0 * 0.02 * 181.0 / 365.0 * 1.05).abs() < 1e-10);
        assert!((second - 1000.0 * 0.03 * 184.0 / 365.0 * 1.10).abs() < 1e-10);
        assert!((varying.bond().redemptions()[0].amount().unwrap() - 1100.0).abs() < 1e-12);
    }
}
