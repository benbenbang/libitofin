//! Native base-date principal observation for live CPI legs.
//!
//! Adapted from QuantLib `ql/cashflows/cpicoupon.cpp`, revision
//! `9863b578af0caa4cecabf697196533e84a8308b6`.
//! Copyright (C) 2009 Chris Kenyon; Copyright (C) 2022 Quaternion Risk Management Ltd.
//! See `THIRD_PARTY_NOTICES.md` and `QUANTLIB_LICENSE.txt`.
use super::cpicoupon::{check_base_cpi, finite_cpi, observed_cpi};
use super::{Coupon, CpiCashFlow};
use crate::cashflow::{CashFlow, cash_flow_has_occurred};
use crate::errors::QlResult;
use crate::event::Event;
use crate::indexes::inflationindex::{CpiInterpolationType, ZeroInflationIndex};
use crate::patterns::observable::{AsObservable, Observable};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::{date::Date, period::Period, timeunit::TimeUnit};

pub(super) struct CpiBaseDateCashFlow {
    flow: CpiCashFlow,
    base_date: Date,
    nominal: f64,
}
impl CpiBaseDateCashFlow {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        nominal: f64,
        index: Shared<ZeroInflationIndex>,
        base_date: Date,
        observation: Date,
        lag: Period,
        interpolation: CpiInterpolationType,
        payment: Date,
        growth_only: bool,
    ) -> QlResult<Self> {
        super::cpicoupon::checked_cpi_shift(base_date, Period::new(0, TimeUnit::Months))?;
        Ok(Self {
            flow: CpiCashFlow::new(
                nominal,
                index,
                1.0,
                observation,
                lag,
                interpolation,
                payment,
                growth_only,
            )?,
            base_date,
            nominal,
        })
    }
}
impl AsObservable for CpiBaseDateCashFlow {
    fn observable(&self) -> &Observable {
        self.flow.observable()
    }
}
impl Event for CpiBaseDateCashFlow {
    fn date(&self) -> Date {
        self.flow.date()
    }
    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        reference: Option<Date>,
        include: Option<bool>,
    ) -> QlResult<bool> {
        cash_flow_has_occurred(self.date(), settings, reference, include)
    }
}
impl CashFlow for CpiBaseDateCashFlow {
    fn amount(&self) -> QlResult<f64> {
        let base = observed_cpi(
            self.flow.cpi_index(),
            self.base_date,
            Period::new(0, TimeUnit::Months),
            self.flow.interpolation(),
        )?;
        check_base_cpi(base)?;
        finite_cpi(
            self.nominal
                * (self.flow.index_fixing()? / base
                    - if self.flow.growth_only() { 1.0 } else { 0.0 }),
        )
    }
    fn ex_coupon_date(&self) -> Option<Date> {
        None
    }
    fn as_coupon(&self) -> Option<&dyn Coupon> {
        None
    }
}
