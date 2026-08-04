//! Cash flow paying a ratio of two lagged inflation fixings.
//!
//! Port of `ql/cashflows/zeroinflationcashflow.{hpp,cpp}`. A
//! [`ZeroInflationCashFlow`] is an [`IndexedCashFlow`] whose two fixings are
//! observed at the start and end dates less an observation lag: with June dates
//! and a three-month lag the ratio is taken between March figures.
//!
//! ## Owning the amount
//!
//! C++ makes the two observations virtual and overrides them, so the base's
//! `performCalculations` picks up the lagged fixings by dispatch. Rust has no
//! such dispatch through composition, so this flow computes its own
//! [`amount`](CashFlow::amount): it reads
//! [`base_fixing`](Self::base_fixing)/[`index_fixing`](Self::index_fixing) -
//! both [`Cpi::lagged_fixing`] - and hands them to the base's ratio. Delegating
//! to [`IndexedCashFlow`]'s `amount` instead would silently price off the
//! base's *raw* fixings: the same number under
//! [`Flat`](CpiInterpolationType::Flat), the wrong one under
//! [`Linear`](CpiInterpolationType::Linear).
//!
//! [`fixing_date`](IndexedCashFlow::fixing_date) is deliberately *not*
//! overridden, matching C++: it reports the raw, unsnapped `end_date - lag`
//! that the base was constructed with, not the start of the period that date
//! falls in.
//!
//! ## Divergences from QuantLib
//!
//! Under [`Flat`](CpiInterpolationType::Flat) the lagged routing has no numeric
//! effect: [`Cpi::lagged_fixing`] reads the fixing of the lagged period, and
//! [`ZeroInflationIndex::fixing`](crate::indexes::Index::fixing) already snaps
//! to that period's first day, so an amount computed off the lagged fixings
//! equals one computed off `fixing(end_date - lag)`. This was checked by
//! delegating [`amount`](CashFlow::amount) to the base: every Flat case here
//! still passed. Only [`Linear`](CpiInterpolationType::Linear) tells the two
//! apart, which is why the routing is pinned there rather than by a Flat
//! mid-period case that would assert nothing.
//!
//! `accept(AcyclicVisitor&)` is unported, as elsewhere in the cash-flow layer.
//! The `growth_only = false` ratio form is ported but untested from the
//! consumer side: the zero-coupon inflation swap this builds towards is
//! growth-only.

use super::indexedcashflow::IndexedCashFlow;
use crate::cashflow::CashFlow;
use crate::cashflows::Coupon;
use crate::errors::QlResult;
use crate::event::Event;
use crate::indexes::inflationindex::{Cpi, CpiInterpolationType, ZeroInflationIndex};
use crate::patterns::observable::{AsObservable, Observable};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::Real;

/// Cash flow dependent on a zero inflation index ratio
/// (`ZeroInflationCashFlow`).
pub struct ZeroInflationCashFlow {
    base: IndexedCashFlow<ZeroInflationIndex>,
    interpolation: CpiInterpolationType,
    start_date: Date,
    end_date: Date,
    observation_lag: Period,
}

impl ZeroInflationCashFlow {
    /// Builds a flow paying `notional` scaled by the inflation growth between
    /// `start_date` and `end_date`, each observed `observation_lag` earlier.
    ///
    /// The base is constructed with the *lagged* dates, `start_date - lag` and
    /// `end_date - lag` (`zeroinflationcashflow.cpp:36-37`), while the unlagged
    /// pair is kept here for the observations themselves.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        notional: Real,
        index: Shared<ZeroInflationIndex>,
        observation_interpolation: CpiInterpolationType,
        start_date: Date,
        end_date: Date,
        observation_lag: Period,
        payment_date: Date,
        growth_only: bool,
    ) -> Self {
        ZeroInflationCashFlow {
            base: IndexedCashFlow::new(
                notional,
                index,
                start_date - observation_lag,
                end_date - observation_lag,
                payment_date,
                growth_only,
            ),
            interpolation: observation_interpolation,
            start_date,
            end_date,
            observation_lag,
        }
    }

    /// The inflation index the ratio is taken on.
    pub fn zero_inflation_index(&self) -> &Shared<ZeroInflationIndex> {
        self.base.index()
    }

    /// How the fixings are observed within their period.
    pub fn observation_interpolation(&self) -> CpiInterpolationType {
        self.interpolation
    }

    /// The unlagged start date, whose observation is the base fixing.
    pub fn start_date(&self) -> Date {
        self.start_date
    }

    /// The unlagged end date, whose observation is the index fixing.
    pub fn end_date(&self) -> Date {
        self.end_date
    }

    /// The lag applied to both dates before observing.
    pub fn observation_lag(&self) -> Period {
        self.observation_lag
    }

    /// The notional the ratio scales.
    pub fn notional(&self) -> Real {
        self.base.notional()
    }

    /// Whether the flow pays the growth rather than the ratio.
    pub fn growth_only(&self) -> bool {
        self.base.growth_only()
    }

    /// The raw `start_date - lag` the base observes at, unsnapped.
    pub fn base_date(&self) -> Date {
        self.base.base_date()
    }

    /// The raw `end_date - lag` the base observes at, unsnapped
    /// (`indexedcashflow.hpp:59`, not overridden by the C++ subclass).
    pub fn fixing_date(&self) -> Date {
        self.base.fixing_date()
    }

    /// The lagged observation at [`start_date`](Self::start_date).
    ///
    /// # Errors
    ///
    /// Propagates whatever [`Cpi::lagged_fixing`] raises.
    pub fn base_fixing(&self) -> QlResult<Real> {
        Cpi::lagged_fixing(
            self.base.index(),
            self.start_date,
            self.observation_lag,
            self.interpolation,
        )
    }

    /// The lagged observation at [`end_date`](Self::end_date).
    ///
    /// # Errors
    ///
    /// Propagates whatever [`Cpi::lagged_fixing`] raises.
    pub fn index_fixing(&self) -> QlResult<Real> {
        Cpi::lagged_fixing(
            self.base.index(),
            self.end_date,
            self.observation_lag,
            self.interpolation,
        )
    }
}

impl AsObservable for ZeroInflationCashFlow {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl Event for ZeroInflationCashFlow {
    fn date(&self) -> Date {
        self.base.date()
    }

    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        ref_date: Option<Date>,
        include_ref_date: Option<bool>,
    ) -> QlResult<bool> {
        self.base.has_occurred(settings, ref_date, include_ref_date)
    }
}

impl CashFlow for ZeroInflationCashFlow {
    fn amount(&self) -> QlResult<Real> {
        Ok(self
            .base
            .amount_from(self.base_fixing()?, self.index_fixing()?))
    }

    fn ex_coupon_date(&self) -> Option<Date> {
        None
    }

    fn as_coupon(&self) -> Option<&dyn Coupon> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexes::Index;
    use crate::indexes::inflation::UkRpi;
    use crate::patterns::observable::Observer;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::time::date::Month::{December, February, January, March, May, November};
    use crate::time::timeunit::TimeUnit;

    const NOTIONAL: Real = 1_000_000.0;

    fn lag() -> Period {
        Period::new(3, TimeUnit::Months)
    }

    fn start_date() -> Date {
        Date::new(10, February, 2021)
    }

    fn end_date() -> Date {
        Date::new(12, May, 2021)
    }

    fn payment_date() -> Date {
        Date::new(26, May, 2021)
    }

    /// The fixture of `testCpiFlatInterpolation` (`inflation.cpp:1346-1360`),
    /// which `ukrpi.rs` already pins the observations of: it is 10 February
    /// 2022 and UK RPI has published November 2020 through March 2021.
    fn a_ukrpi_with_2021_fixings() -> Shared<ZeroInflationIndex> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(Date::new(10, February, 2022));
        let index = shared(UkRpi::new(settings));
        for (date, value) in [
            (Date::new(1, November, 2020), 293.5),
            (Date::new(1, December, 2020), 295.4),
            (Date::new(1, January, 2021), 294.6),
            (Date::new(1, February, 2021), 296.0),
            (Date::new(1, March, 2021), 296.9),
        ] {
            index.add_fixing(date, value).expect("a published figure");
        }
        index
    }

    fn a_flow_with(
        interpolation: CpiInterpolationType,
        growth_only: bool,
    ) -> ZeroInflationCashFlow {
        ZeroInflationCashFlow::new(
            NOTIONAL,
            a_ukrpi_with_2021_fixings(),
            interpolation,
            start_date(),
            end_date(),
            lag(),
            payment_date(),
            growth_only,
        )
    }

    fn a_flow(growth_only: bool) -> ZeroInflationCashFlow {
        a_flow_with(CpiInterpolationType::Flat, growth_only)
    }

    /// The swap-type flow, which is what a zero-coupon inflation swap pays.
    ///
    /// The two observations are the ones `ukrpi.rs`'s
    /// `a_flat_observation_reads_the_lagged_period` pins independently: 10
    /// February 2021 less three months lands in November 2020, so `i0 = 293.5`;
    /// 12 May 2021 lands in February 2021, so `i1 = 296.0`. The amount is then
    /// `1e6 * (296.0 / 293.5 - 1) = 8517.887563884053`.
    #[test]
    fn a_zero_inflation_cash_flow_pays_the_lagged_inflation_growth() {
        let flow = a_flow(true);

        assert!((flow.base_fixing().unwrap() - 293.5).abs() < 1e-8);
        assert!((flow.index_fixing().unwrap() - 296.0).abs() < 1e-8);
        assert!((flow.amount().unwrap() - 8517.887563884053).abs() < 1e-8);
    }

    /// The one case that tells the lagged routing apart from the base's raw
    /// fixings, and so the guard on this flow owning its own amount.
    ///
    /// Both observations are the ones `ukrpi.rs`'s
    /// `a_linear_observation_interpolates_the_bracketing_fixings` pins
    /// independently: `293.5 * (19/28) + 295.4 * (9/28) = 294.1107142857143`
    /// and `296.0 * (20/31) + 296.9 * (11/31) = 296.31935483870967`, so the
    /// amount is `1e6 * (296.31935483870967 / 294.1107142857143 - 1) =
    /// 7509.554891120818`. An `amount` delegated to [`IndexedCashFlow`] would
    /// read the snapped 293.5 and 296.0 and pay 8517.887563884053 instead.
    #[test]
    fn a_linear_flow_pays_the_interpolated_growth_not_the_raw_one() {
        let flow = a_flow_with(CpiInterpolationType::Linear, true);

        let i0 = 293.5 * (19.0 / 28.0) + 295.4 * (9.0 / 28.0);
        let i1 = 296.0 * (20.0 / 31.0) + 296.9 * (11.0 / 31.0);
        assert!((flow.base_fixing().unwrap() - i0).abs() < 1e-8);
        assert!((flow.index_fixing().unwrap() - i1).abs() < 1e-8);
        assert!((flow.amount().unwrap() - 7509.554891120818).abs() < 1e-8);
    }

    /// The bond-type flow: the same two observations, full ratio.
    #[test]
    fn a_zero_inflation_cash_flow_can_pay_the_full_ratio() {
        let flow = a_flow(false);

        assert!(!flow.growth_only());
        assert!((flow.amount().unwrap() - 1_008_517.887563884).abs() < 1e-8);
    }

    /// `zeroinflationcashflow.cpp:36-37` and `indexedcashflow.hpp:59`: the base
    /// is built on the lagged dates, and `fixingDate()` reports that date raw.
    /// A port that snapped it to its inflation period would answer 1 February
    /// 2021 here, and 1 November 2020 for the base date.
    #[test]
    fn the_observation_dates_are_lagged_but_not_snapped() {
        let flow = a_flow(true);

        assert_eq!(flow.start_date(), start_date());
        assert_eq!(flow.end_date(), end_date());
        assert_eq!(flow.observation_lag(), lag());
        assert_eq!(flow.notional(), NOTIONAL);
        assert_eq!(flow.observation_interpolation(), CpiInterpolationType::Flat);

        assert_eq!(flow.base_date(), Date::new(10, November, 2020));
        assert_eq!(flow.fixing_date(), Date::new(12, February, 2021));
        assert_eq!(flow.date(), payment_date());
        assert_eq!(flow.ex_coupon_date(), None);
        assert!(flow.as_coupon().is_none());
    }

    /// The flow broadcasts through the base's observable, the one registered
    /// with the index. A second observable of its own would be fed by nobody.
    #[test]
    fn a_zero_inflation_cash_flow_forwards_its_index_notifications() {
        #[derive(Default)]
        struct Flag {
            up: bool,
        }
        impl Observer for Flag {
            fn update(&mut self) {
                self.up = true;
            }
        }

        let flow = a_flow(true);
        let flag = shared_mut(Flag::default());
        flow.observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        Index::observable(&**flow.zero_inflation_index()).notify_observers();
        assert!(flag.borrow().up);
    }
}
