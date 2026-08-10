//! Inflation term structure based on the interpolation of year-on-year rates.
//!
//! Port of `ql/termstructures/inflation/interpolatedyoyinflationcurve.hpp`
//! (header-only): [`InterpolatedYoYInflationCurve`] builds a
//! [`YoYInflationTermStructure`] from (date, year-on-year rate) nodes
//! interpolated in rate space, composing the shared
//! [`InflationTermStructureBase`] holder with an [`InterpolatedCurve`];
//! [`YoYInflationCurve`] is the C++ `typedef` for the linearly interpolated
//! case (`interpolatedyoyinflationcurve.hpp:87`).
//!
//! The quoted rates are *not* year-on-year inflation-swap quotes
//! (`interpolatedyoyinflationcurve.hpp:37`); a curve fitted to those is
//! [`PiecewiseYoYInflationCurve`](super::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve).
//!
//! It mirrors
//! [`InterpolatedZeroInflationCurve`](super::interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve)
//! throughout, including the node times being measured from the *reference
//! date* rather than from the first node, which leaves the first time negative.
//! The one divergence between the two is the base rate: C++ forwards
//! `rates[0]` into the base constructor's `baseRate` slot
//! (`interpolatedyoyinflationcurve.hpp:99-100`) where the zero curve passes
//! nothing, so [`base_rate`](InflationTermStructure::base_rate) succeeds here
//! and errors there.
//!
//! ## Divergences from QuantLib
//!
//! - The protected constructor for descendants that cannot supply their nodes
//!   at construction (`interpolatedyoyinflationcurve.hpp:79-85,124-133`) is
//!   omitted rather than deferred: it exists in C++ only because
//!   `PiecewiseYoYInflationCurve` *derives* from this class, and this port's
//!   piecewise curve is standalone (as its zero sibling is), so nothing could
//!   call it.
//! - The seasonality constructor argument (`:50`) is ported, but the
//!   consistency gate C++ runs from the base constructor runs here from this
//!   one, once the curve exists; see the
//!   [`inflationtermstructure`](super::inflationtermstructure) divergences.
//! - C++ reads `dates.at(0)` in the initializer list *before* checking the
//!   size, throwing `out_of_range` on an empty vector; here the size check
//!   runs first and every input problem is a `QlError` per D4.
//! - [`yoy_rate_impl`](YoYInflationTermStructure::yoy_rate_impl) evaluates the
//!   interpolation without extrapolation where C++ always extrapolates
//!   (`interpolatedyoyinflationcurve.hpp:149` passes `true`): the extrapolation
//!   flag is carried per interpolation object and set at construction, so it
//!   cannot be flipped through a generic [`Interpolator::Output`] - the
//!   limitation already documented on the zero curve. The gap is bounded
//!   exactly as it is there: the inflation range checks admit
//!   `[base_date, max_date]`, whose ends are the interpolation's own `x_min`
//!   (the base date *is* the first node) and `x_max` (the maximum date *is* the
//!   last node), so every non-extrapolating query matches C++ and only an
//!   explicit `extrapolate = true` or an
//!   [`enable_extrapolation`](TermStructure::enable_extrapolation) past the
//!   last node errors where C++ extends the end segment.

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::shared::Shared;
use crate::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, InflationTermStructureBase, YoYInflationTermStructure,
};
use crate::termstructures::inflation::seasonality::Seasonality;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};
use crate::{fail, require};

/// Inflation term structure based on the interpolation of year-on-year rates.
///
/// The first date is the base date, the last date for which the fixing is
/// known; the reference date is passed separately and normally follows it. The
/// first rate is the base rate the curve publishes.
pub struct InterpolatedYoYInflationCurve<I: Interpolator> {
    inflation: InflationTermStructureBase,
    curve: InterpolatedCurve<I>,
    dates: Vec<Date>,
}

/// Inflation term structure based on linear interpolation of year-on-year
/// rates (C++'s `YoYInflationCurve` typedef).
pub type YoYInflationCurve = InterpolatedYoYInflationCurve<Linear>;

impl<I: Interpolator> InterpolatedYoYInflationCurve<I> {
    /// Curve through the year-on-year inflation rates quoted at the given
    /// dates, the first of which is the base date
    /// (`interpolatedyoyinflationcurve.hpp:93-121`).
    ///
    /// The validations run in C++'s order, so their precedence is preserved.
    /// The bound on the rates starts at the *second* node (`:112`), leaving the
    /// base rate unconstrained, and admits negative rates down to but not
    /// including `-1.0`: year-on-year inflation may be negative, but a fall of
    /// a hundred per cent or more is not a rate. It is spelled as the negation
    /// C++'s `QL_REQUIRE` reduces to, so `NaN` fails it exactly as it does
    /// there.
    pub fn new(
        reference_date: Date,
        dates: Vec<Date>,
        rates: Vec<Rate>,
        frequency: Frequency,
        day_counter: DayCounter,
        interpolator: I,
        seasonality: Option<Shared<dyn Seasonality>>,
    ) -> QlResult<InterpolatedYoYInflationCurve<I>> {
        require!(dates.len() > 1, "too few dates: {}", dates.len());
        require!(
            rates.len() == dates.len(),
            "indices/dates count mismatch: {} vs {}",
            rates.len(),
            dates.len()
        );
        for &rate in &rates[1..] {
            if rate <= -1.0 || rate.is_nan() {
                fail!("year-on-year inflation data < -100 %");
            }
        }

        let base_date = dates[0];
        let base_rate = rates[0];
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, reference_date, &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, rates, interpolator);
        curve.setup_interpolation()?;
        let inflation = InflationTermStructureBase::with_reference_date(
            reference_date,
            base_date,
            frequency,
            Some(day_counter),
            Some(base_rate),
            seasonality,
        );
        let curve = InterpolatedYoYInflationCurve {
            inflation,
            curve,
            dates,
        };
        curve.check_seasonality()?;
        Ok(curve)
    }

    /// The node times, measured from the reference date; the first is
    /// negative whenever the base date precedes it.
    pub fn times(&self) -> &[Time] {
        self.curve.times()
    }

    /// The node dates, the first of which is the base date.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node values (year-on-year inflation rates).
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node values (year-on-year inflation rates).
    pub fn rates(&self) -> &[Rate] {
        self.curve.data()
    }

    /// The curve nodes as (date, year-on-year rate) pairs.
    pub fn nodes(&self) -> Vec<(Date, Rate)> {
        self.dates
            .iter()
            .copied()
            .zip(self.curve.data().iter().copied())
            .collect()
    }

    fn last_date(&self) -> Date {
        *self
            .dates
            .last()
            .expect("construction rejected empty dates")
    }
}

impl<I: Interpolator> AsObservable for InterpolatedYoYInflationCurve<I> {
    fn observable(&self) -> &Observable {
        self.inflation.term_structure_base().observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedYoYInflationCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.inflation.term_structure_base()
    }

    fn max_date(&self) -> Date {
        self.curve.max_date().unwrap_or_else(|| self.last_date())
    }
}

impl<I: Interpolator> InflationTermStructure for InterpolatedYoYInflationCurve<I> {
    fn inflation_base(&self) -> &InflationTermStructureBase {
        &self.inflation
    }

    fn as_inflation_term_structure(&self) -> &dyn InflationTermStructure {
        self
    }
}

impl<I: Interpolator> YoYInflationTermStructure for InterpolatedYoYInflationCurve<I> {
    fn yoy_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.curve.interpolation()?.value(t)
    }
}
