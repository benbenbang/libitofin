//! Piecewise-bootstrapped year-on-year inflation term structure.
//!
//! Port of `ql/termstructures/inflation/piecewiseyoyinflationcurve.hpp:37-154`.
//! A [`PiecewiseYoYInflationCurve`] is built from [`YoYInflationHelper`]s - in
//! practice year-on-year inflation swaps - whose observed fixing periods mark
//! the segment boundaries; each node is solved so the helper reprices its
//! quoted rate off the curve, by the same [`IterativeBootstrap`] the zero,
//! yield and credit sides run.
//!
//! It mirrors
//! [`PiecewiseZeroInflationCurve`](super::piecewisezeroinflationcurve::PiecewiseZeroInflationCurve)
//! field for field. C++ derives from `InterpolatedYoYInflationCurve<Interpolator>`
//! *and* `LazyObject` (`:40-42`); Rust has no inheritance, so the node storage
//! lives here and [`yoy_rate_impl`](YoYInflationTermStructure::yoy_rate_impl)
//! reads it the way [`InterpolatedYoYInflationCurve`] reads its own
//! (`interpolatedyoyinflationcurve.hpp:147-150`).
//!
//! [`InterpolatedYoYInflationCurve`]: super::interpolatedyoyinflationcurve::InterpolatedYoYInflationCurve
//!
//! ## Node zero carries the base rate, and keeps it
//!
//! This is the difference from the zero curve that the whole
//! [`YoYInflationTraits`] transcription exists for, and it takes two halves
//! that only work together. C++'s `YoYInflationTraits::initialValue` reads the
//! curve's *own* base rate off the term-structure pointer it is handed
//! (`inflationtraits.hpp:129-131`) where the zero convention returns a dummy
//! constant (`:50-53`), so this curve overrides
//! [`PiecewiseCurve::initial_value`] with
//! [`base_rate`](InflationTermStructure::base_rate) - the hook existing for
//! exactly this case. And C++'s `YoYInflationTraits::updateGuess` writes only
//! node `i` (`:175-179`) where the zero convention also mirrors the first
//! solved pillar onto node 0 (`:100-102`), so the seeded base rate is never
//! overwritten. Seed the node from the traits constant instead, or mirror
//! pillar 1 onto it, and the curve publishes an invented figure at its base
//! date in place of a quoted one.
//!
//! The base *date* anchoring node zero is the zero curve's behaviour unchanged:
//! `initialDate` is the curve's `baseDate()` on both conventions (`:124-126`
//! against `:46-48`), so the first node precedes the reference date and its
//! time is negative.
//!
//! ## Divergences from QuantLib
//!
//! - The seasonality argument (`:61`) is ported, and so is C++'s virtual
//!   `update()` behind
//!   [`set_seasonality`](InflationTermStructure::set_seasonality): installing
//!   one invalidates the bootstrap, so the next read re-solves every node
//!   against the corrected rates. The consistency gate runs from this
//!   constructor rather than from the base one; see the
//!   [`inflationtermstructure`](super::inflationtermstructure) divergences.
//! - The `accuracy` constructor argument (`:62`) is not exposed, matching the
//!   zero curve; the field carries the C++ default, which is `1.0e-12` here
//!   against the zero curve's `1.0e-14`.
//! - Only [`Linear`] is constructible ([`new`](PiecewiseYoYInflationCurve::new)
//!   builds the interpolator itself), so the C++ `Interpolator` argument
//!   (`:63`) has no counterpart. The impls below are generic, so a second
//!   constructor is all another local interpolator needs.

use std::cell::RefCell;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::termstructures::bootstraptraits::CurveData;
use crate::termstructures::inflation::inflationhelpers::YoYInflationHelper;
use crate::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, InflationTermStructureBase, YoYInflationTermStructure,
};
use crate::termstructures::inflation::inflationtraits::YoYInflationTraits;
use crate::termstructures::inflation::seasonality::Seasonality;
use crate::termstructures::iterativebootstrap::{IterativeBootstrap, PiecewiseCurve};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// Feeds a helper-quote or evaluation-date notification into the curve's lazy
/// core: it invalidates the bootstrap cache and re-broadcasts to the curve's own
/// observers (the port of `update()`, `:145-149`).
struct CurveUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for CurveUpdater {
    fn update(&mut self) {
        if let Some(update) = LazyObject::deferred_update(&self.lazy) {
            update.notify_observers();
        }
    }
}

/// Year-on-year inflation term structure bootstrapped from inflation helpers.
///
/// `I` is the interpolation factory ([`Linear`]); the curve shape traits are
/// always [`YoYInflationTraits`], the nodes being the year-on-year inflation
/// rates themselves. The node data lives in a `RefCell` the bootstrap mutates
/// and the rate lookups read back.
pub struct PiecewiseYoYInflationCurve<I: Interpolator> {
    inflation: InflationTermStructureBase,
    instruments: Vec<Shared<dyn YoYInflationHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: IterativeBootstrap,
    accuracy: Real,
    self_weak: Weak<dyn YoYInflationTermStructure>,
}

impl PiecewiseYoYInflationCurve<Linear> {
    /// Builds a linearly interpolated curve over `instruments` with a fixed
    /// `reference_date` (`:54-73`). Construction is cheap; the bootstrap runs on
    /// first use.
    ///
    /// `base_date` is the last date for which a fixing is known and precedes
    /// the reference date; `base_yoy_rate` is the year-on-year rate observed
    /// over the period ending there, and is what node zero is seeded with and
    /// keeps.
    ///
    /// # Errors
    ///
    /// Rejects an empty helper set.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        reference_date: Date,
        base_date: Date,
        base_yoy_rate: Rate,
        frequency: Frequency,
        day_counter: DayCounter,
        instruments: Vec<Shared<dyn YoYInflationHelper>>,
        seasonality: Option<Shared<dyn Seasonality>>,
    ) -> QlResult<Shared<PiecewiseYoYInflationCurve<Linear>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(|weak: &Weak<PiecewiseYoYInflationCurve<Linear>>| {
            let self_weak: Weak<dyn YoYInflationTermStructure> = weak.clone();
            let lazy = shared_mut(LazyObject::new(true));
            let observable = lazy.borrow().observable_handle();
            let updater = shared_mut(CurveUpdater {
                lazy: SharedMut::clone(&lazy),
            });
            PiecewiseYoYInflationCurve {
                inflation: InflationTermStructureBase::with_reference_date(
                    reference_date,
                    base_date,
                    frequency,
                    Some(day_counter),
                    Some(base_yoy_rate),
                    seasonality,
                ),
                instruments,
                interpolator: Linear,
                data: RefCell::new(CurveData::new()),
                lazy,
                observable,
                updater,
                bootstrap: IterativeBootstrap::new(),
                accuracy: 1.0e-12,
                self_weak,
            }
        });

        let observer = SharedMut::clone(&curve.updater) as SharedMut<dyn Observer>;
        for helper in &curve.instruments {
            helper.observable().register_observer(&observer);
        }
        curve.check_seasonality()?;
        Ok(curve)
    }
}

impl<I: Interpolator + 'static> PiecewiseYoYInflationCurve<I> {
    /// Runs the bootstrap if the cache is stale, caching the result
    /// (`performCalculations`, `:139-142`). The lazy core is not borrowed while
    /// the bootstrap runs, so a helper reading the curve mid-bootstrap - which
    /// every one of them does, through the index it forecasts with - re-enters
    /// here and returns on the pre-set flag, answering off the partially solved
    /// node prefix.
    pub fn calculate(&self) -> QlResult<()> {
        if self.lazy.borrow().is_calculated() {
            return Ok(());
        }
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.bootstrap.calculate(self);
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    /// The node times, after bootstrapping (`:114-118`). The first is negative,
    /// the base date preceding the reference date.
    pub fn times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.data.borrow().times().to_vec())
    }

    /// The node dates, after bootstrapping (`:120-124`). The first is the base
    /// date.
    pub fn dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.data.borrow().dates().to_vec())
    }

    /// The node year-on-year inflation rates, after bootstrapping (`:126-130`).
    /// The first is the base rate the curve was built with.
    pub fn data(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.data.borrow().data().to_vec())
    }

    /// The (date, year-on-year rate) nodes, after bootstrapping (`:132-137`).
    pub fn nodes(&self) -> QlResult<Vec<(Date, Real)>> {
        self.calculate()?;
        Ok(self.data.borrow().nodes())
    }

    /// Registers a downstream observer of the curve's notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }
}

impl<I: Interpolator> AsObservable for PiecewiseYoYInflationCurve<I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<I: Interpolator + 'static> TermStructure for PiecewiseYoYInflationCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.inflation.term_structure_base()
    }

    fn max_date(&self) -> Date {
        // Trigger the bootstrap so the maximum reflects the solved curve
        // (`:109-112`); a bootstrap failure is surfaced by the reads, so fall
        // back here.
        let _ = self.calculate();
        self.data
            .borrow()
            .max_date()
            .or_else(|| self.inflation.term_structure_base().reference_date().ok())
            .unwrap_or_else(Date::null)
    }
}

impl<I: Interpolator + 'static> InflationTermStructure for PiecewiseYoYInflationCurve<I> {
    fn inflation_base(&self) -> &InflationTermStructureBase {
        &self.inflation
    }

    fn as_inflation_term_structure(&self) -> &dyn InflationTermStructure {
        self
    }

    /// Invalidates the bootstrap before broadcasting: a seasonality change
    /// moves the rates the helpers reprice against, so the solved nodes are
    /// stale and every one of them has to be solved again. This is the curve's
    /// own `update()` (`:145-149`), which the base default cannot reach.
    fn update_after_seasonality_change(&self) {
        if let Some(update) = LazyObject::deferred_update(&self.lazy) {
            update.notify_observers();
        }
    }
}

impl<I: Interpolator + 'static> YoYInflationTermStructure for PiecewiseYoYInflationCurve<I> {
    fn yoy_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.calculate()?;
        let data = self.data.borrow();
        data.interpolation()?.value(t)
    }
}

impl<I: Interpolator + 'static> PiecewiseCurve for PiecewiseYoYInflationCurve<I> {
    type Traits = YoYInflationTraits;
    type Interp = I;
    type TS = dyn YoYInflationTermStructure;
    type Helper = dyn YoYInflationHelper;

    fn instruments(&self) -> &[Shared<dyn YoYInflationHelper>] {
        &self.instruments
    }

    fn interpolator(&self) -> &I {
        &self.interpolator
    }

    fn curve_data(&self) -> &RefCell<CurveData<I>> {
        &self.data
    }

    fn accuracy(&self) -> Real {
        self.accuracy
    }

    fn reference_date(&self) -> QlResult<Date> {
        self.inflation.term_structure_base().reference_date()
    }

    /// The base date, where every other piecewise curve answers its reference
    /// date (`YoYInflationTraits::initialDate`, `inflationtraits.hpp:124-126`).
    fn initial_date(&self) -> QlResult<Date> {
        Ok(InflationTermStructure::base_date(self))
    }

    /// The curve's own base rate, where every other piecewise curve - the
    /// zero-inflation one included - takes the traits' constant
    /// (`YoYInflationTraits::initialValue`, `inflationtraits.hpp:129-131`).
    /// Node zero holds this figure for the life of the bootstrap, the
    /// year-on-year `update_guess` never writing to it.
    fn initial_value(&self) -> QlResult<Real> {
        InflationTermStructure::base_rate(self)
    }

    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        TermStructure::time_from_reference(self, date)
    }

    fn term_structure_shared(&self) -> QlResult<Shared<dyn YoYInflationTermStructure>> {
        match self.self_weak.upgrade() {
            Some(curve) => Ok(curve),
            None => crate::fail!("curve dropped before bootstrap"),
        }
    }
}

#[cfg(test)]
mod tests {
    //! `inflation.cpp`'s `testYYTermStructure` (`:1109`) is the only
    //! year-on-year curve QuantLib builds anywhere, and it needs the
    //! year-on-year swap and its helper; that reprice-to-zero against C++'s own
    //! numbers is the oracle of the batch that lands them. What is checked here
    //! is what this curve decides on its own, against a helper written to make
    //! those decisions visible.
    //!
    //! The mock reprices off *two* points of the curve, its own pillar and the
    //! base node, so its solved value is a function of the base rate rather
    //! than of its quote alone. That is what a real year-on-year helper does -
    //! a swap's fair rate depends on the whole curve prefix - and it is what
    //! turns the seeding of node zero into something a repricing test can see:
    //! with `implied = (curve(pillar) + curve(base)) / 2`, the solved pillar is
    //! `2 * quote - base_rate`, so a curve seeded from the traits' `0.02`
    //! instead of its own base rate lands every node somewhere else while still
    //! repricing its quotes perfectly.

    use super::*;
    use crate::handle::Handle;
    use crate::patterns::observable::AsObservable;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::shared;
    use crate::termstructures::inflation::inflationhelpers::YoYInflationHelperBase;
    use crate::time::date::Month::{August, July};
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    /// Distinct from the traits' `0.02` seed and from every solved pillar
    /// below, in the spirit of C++'s `baseYYRate = yyData[0] / 100 = 0.0295`
    /// (`inflation.cpp:1181`). A base rate of `0.02` would let a curve that
    /// ignored the `initial_value` override pass every assertion here.
    const BASE_YOY_RATE: Rate = 0.029;
    const QUOTES: [Real; 3] = [0.026, 0.028, 0.030];
    const MATURITY_YEARS: [i32; 3] = [1, 3, 5];

    fn today() -> Date {
        Date::new(13, August, 2007)
    }

    fn base_date() -> Date {
        Date::new(1, July, 2007)
    }

    fn day_counter() -> DayCounter {
        Thirty360::with_convention(Convention::BondBasis)
    }

    /// A helper whose implied quote is the mean of the curve's year-on-year
    /// rate at its pillar and at the base date.
    ///
    /// It reads the curve by *time*, not by date: the date entry point
    /// quantizes to the start of the query's inflation period, which for
    /// pillars that are not month starts would land the read between two nodes
    /// and blunt the solver's sensitivity to the node it is solving. A test
    /// double is free to take the unquantized path; the curve is not.
    struct MeanRateHelper {
        base: YoYInflationHelperBase,
    }

    impl MeanRateHelper {
        fn new(quote: &Shared<SimpleQuote>, pillar: Date) -> Shared<MeanRateHelper> {
            let base =
                YoYInflationHelperBase::new(Handle::new(Shared::clone(quote) as Shared<dyn Quote>));
            base.set_pillar_date(pillar);
            base.set_latest_relevant_date(pillar);
            base.set_maturity_date(pillar);
            shared(MeanRateHelper { base })
        }
    }

    impl AsObservable for MeanRateHelper {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl YoYInflationHelper for MeanRateHelper {
        fn base(&self) -> &YoYInflationHelperBase {
            &self.base
        }

        /// Extrapolation is requested because the curve is only partially
        /// solved while the bootstrap is running; both reads sit on nodes of
        /// the solved prefix, so nothing is ever extrapolated in fact.
        fn implied_quote(&self) -> QlResult<Real> {
            let curve = self.base.term_structure()?;
            let at_pillar = curve.time_from_reference(self.base.pillar_date())?;
            let at_base = curve.time_from_reference(curve.base_date())?;
            Ok(0.5 * (curve.yoy_rate(at_pillar, true)? + curve.yoy_rate(at_base, true)?))
        }
    }

    struct Fixture {
        helpers: Vec<Shared<dyn YoYInflationHelper>>,
        curve: Shared<PiecewiseYoYInflationCurve<Linear>>,
    }

    fn a_curve() -> Fixture {
        let helpers: Vec<Shared<dyn YoYInflationHelper>> = QUOTES
            .iter()
            .zip(MATURITY_YEARS)
            .map(|(quote, years)| {
                MeanRateHelper::new(
                    &shared(SimpleQuote::new(Some(*quote))),
                    today() + Period::new(years, TimeUnit::Years),
                ) as Shared<dyn YoYInflationHelper>
            })
            .collect();
        let curve = PiecewiseYoYInflationCurve::new(
            today(),
            base_date(),
            BASE_YOY_RATE,
            Frequency::Monthly,
            day_counter(),
            helpers.clone(),
            None,
        )
        .unwrap();
        Fixture { helpers, curve }
    }

    /// Every helper reprices its quote off the bootstrapped curve, and the
    /// pillars land where the helpers put them.
    #[test]
    fn the_bootstrapped_curve_reproduces_every_helpers_quote() {
        let fixture = a_curve();
        let (helpers, curve) = (&fixture.helpers, &fixture.curve);
        curve.calculate().unwrap();

        for (helper, quote) in helpers.iter().zip(QUOTES) {
            let error = helper.quote_error().unwrap();
            assert!(
                error.abs() < 1.0e-10,
                "quote error {error} on the {quote} helper"
            );
        }

        let dates = curve.dates().unwrap();
        assert_eq!(
            dates.len(),
            helpers.len() + 1,
            "one node per helper, plus the base-date node"
        );
        assert_eq!(dates[0], base_date());
        for (i, helper) in helpers.iter().enumerate() {
            assert_eq!(dates[i + 1], helper.pillar_date());
        }
    }

    /// The discriminator of the two halves of the year-on-year convention: node
    /// zero is seeded from the curve's own base rate through the
    /// `initial_value` override, and `YoYInflationTraits::update_guess` never
    /// writes to it, so it still holds that rate bit for bit once every pillar
    /// is solved. A curve seeded from the traits' constant, or one whose traits
    /// mirrored the first solved pillar onto node zero the way the
    /// zero-inflation convention does, fails on the first assertion; the
    /// solved-value pin that follows fails on the seeding independently, the
    /// helpers reading node zero back.
    #[test]
    fn the_base_node_keeps_the_curves_own_base_rate_through_the_bootstrap() {
        let curve = a_curve().curve;
        let data = curve.data().unwrap();

        assert_eq!(data[0], BASE_YOY_RATE);
        for (i, quote) in QUOTES.iter().enumerate() {
            let expected = 2.0 * quote - BASE_YOY_RATE;
            assert!(
                (data[i + 1] - expected).abs() < 1.0e-10,
                "node {} solved to {} against {expected}",
                i + 1,
                data[i + 1]
            );
            assert_ne!(data[i + 1], data[0], "the fixture must not be degenerate");
        }
        assert_ne!(
            data[0], 0.02,
            "a 0.02 base rate would hide a seeding mis-wire"
        );
    }

    /// Between two pillars the curve is the linear interpolant of the solved
    /// nodes, read at a time so the query is not quantized to a period start.
    #[test]
    fn the_curve_interpolates_linearly_between_the_solved_pillars() {
        let curve = a_curve().curve;
        let (times, data) = (curve.times().unwrap(), curve.data().unwrap());

        let midpoint = 0.5 * (times[1] + times[2]);
        let expected = 0.5 * (data[1] + data[2]);
        assert!((curve.yoy_rate(midpoint, false).unwrap() - expected).abs() < 1.0e-12);
        assert!((curve.yoy_rate(times[0], false).unwrap() - BASE_YOY_RATE).abs() < 1.0e-12);
    }

    /// Construction lays down no nodes and runs no solver; the first read
    /// bootstraps; a quote move invalidates the cache and the next read
    /// re-bootstraps higher (the `LazyObject` contract, and the [`CurveUpdater`]
    /// registration that carries the quote's notification).
    #[test]
    fn the_bootstrap_is_lazy_and_reruns_on_a_quote_change() {
        let quote = shared(SimpleQuote::new(Some(0.026)));
        let helper = MeanRateHelper::new(&quote, today() + Period::new(5, TimeUnit::Years));
        let curve = PiecewiseYoYInflationCurve::new(
            today(),
            base_date(),
            BASE_YOY_RATE,
            Frequency::Monthly,
            day_counter(),
            vec![Shared::clone(&helper) as Shared<dyn YoYInflationHelper>],
            None,
        )
        .unwrap();

        assert!(!curve.lazy.borrow().is_calculated());
        let first = curve.data().unwrap()[1];
        assert!(curve.lazy.borrow().is_calculated());
        assert!((first - (2.0 * 0.026 - BASE_YOY_RATE)).abs() < 1.0e-10);

        quote.set_value(Some(0.04));
        assert!(!curve.lazy.borrow().is_calculated());
        assert!(
            curve.data().unwrap()[1] > first,
            "a higher quoted rate must lift the curve"
        );
    }

    #[test]
    fn an_empty_helper_set_is_rejected() {
        let built = PiecewiseYoYInflationCurve::new(
            today(),
            base_date(),
            BASE_YOY_RATE,
            Frequency::Monthly,
            day_counter(),
            Vec::new(),
            None,
        );
        let err = match built {
            Ok(_) => panic!("expected a construction error"),
            Err(err) => err,
        };
        assert!(err.message().contains("no bootstrap helpers"));
    }
}
