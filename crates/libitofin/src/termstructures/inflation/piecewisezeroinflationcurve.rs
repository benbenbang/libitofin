//! Piecewise-bootstrapped zero-coupon inflation term structure.
//!
//! Port of `ql/termstructures/inflation/piecewisezeroinflationcurve.hpp:55-72`.
//! A [`PiecewiseZeroInflationCurve`] is built from
//! [`ZeroInflationHelper`]s - in practice zero-coupon inflation swaps - whose
//! observed fixing periods mark the segment boundaries; each node is solved so
//! the helper reprices its quoted rate off the curve, by the same
//! [`IterativeBootstrap`] the yield and credit sides run.
//!
//! It mirrors
//! [`PiecewiseDefaultCurve`](crate::termstructures::credit::piecewisedefaultcurve::PiecewiseDefaultCurve)
//! field for field: the laziness contract, the pre-set `calculated` flag, the
//! observer registration on every helper and the [`PiecewiseCurve`] surface the
//! bootstrap drives are all the same. C++ derives from
//! `InterpolatedZeroInflationCurve<Interpolator>` *and* `LazyObject`
//! (`:56-57`); Rust has no inheritance, so the node storage lives here and
//! [`zero_rate_impl`](ZeroInflationTermStructure::zero_rate_impl) reads it the
//! way [`InterpolatedZeroInflationCurve`] reads its own
//! (`interpolatedzeroinflationcurve.hpp:137`).
//!
//! ## The base date, not the reference date, anchors node zero
//!
//! This is the one structural difference from every other piecewise curve.
//! C++'s `ZeroInflationTraits::initialDate` returns the curve's `baseDate()`
//! (`inflationtraits.hpp:46-48`), the last date for which a fixing is known,
//! which *precedes* the reference date. The port carries that decision on the
//! curve as [`PiecewiseCurve::initial_date`], overridden below, so the first
//! bootstrap node lands on the base date while `time_from_reference` stays
//! anchored at the reference date - leaving `times()[0]` negative.
//!
//! [`InterpolatedZeroInflationCurve`]: super::interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve
//!
//! ## Divergences from QuantLib
//!
//! - Seasonality (`:61`) is omitted rather than accepted and ignored, following
//!   the [`inflationtermstructure`](super::inflationtermstructure) contract: a
//!   caller needing it fails to compile. It follows with the seasonality
//!   classes in EPIC Inflation (#705).
//! - The `BaseDateFunc` constructor overload (`:73-90`), whose base date is
//!   resolved lazily inside `performCalculations` (`:167-169`), is deferred
//!   with it; the base date here is always the caller's.
//! - Only [`Linear`] is constructible ([`new`](PiecewiseZeroInflationCurve::new)
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
use crate::termstructures::inflation::inflationhelpers::ZeroInflationHelper;
use crate::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, InflationTermStructureBase, ZeroInflationTermStructure,
};
use crate::termstructures::inflation::inflationtraits::ZeroInflationTraits;
use crate::termstructures::iterativebootstrap::{IterativeBootstrap, PiecewiseCurve};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// Feeds a helper-quote or evaluation-date notification into the curve's lazy
/// core: it invalidates the bootstrap cache and re-broadcasts to the curve's own
/// observers (the port of `update()`, `:180-183`).
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

/// Zero-coupon inflation term structure bootstrapped from inflation helpers.
///
/// `I` is the interpolation factory ([`Linear`]); the curve shape traits are
/// always [`ZeroInflationTraits`], the nodes being the zero-coupon inflation
/// rates themselves. The node data lives in a `RefCell` the bootstrap mutates
/// and the zero-rate lookups read back.
pub struct PiecewiseZeroInflationCurve<I: Interpolator> {
    inflation: InflationTermStructureBase,
    instruments: Vec<Shared<dyn ZeroInflationHelper>>,
    interpolator: I,
    data: RefCell<CurveData<I>>,
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
    updater: SharedMut<CurveUpdater>,
    bootstrap: IterativeBootstrap,
    accuracy: Real,
    self_weak: Weak<dyn ZeroInflationTermStructure>,
}

impl PiecewiseZeroInflationCurve<Linear> {
    /// Builds a linearly interpolated curve over `instruments` with a fixed
    /// `reference_date` (`:69-84`). Construction is cheap; the bootstrap runs on
    /// first use.
    ///
    /// `base_date` is the last date for which a fixing is known - in practice
    /// [`ZeroInflationIndex::last_fixing_date`] - and precedes the reference
    /// date.
    ///
    /// # Errors
    ///
    /// Rejects an empty helper set.
    ///
    /// [`ZeroInflationIndex::last_fixing_date`]: crate::indexes::inflationindex::ZeroInflationIndex::last_fixing_date
    pub fn new(
        reference_date: Date,
        base_date: Date,
        frequency: Frequency,
        day_counter: DayCounter,
        instruments: Vec<Shared<dyn ZeroInflationHelper>>,
    ) -> QlResult<Shared<PiecewiseZeroInflationCurve<Linear>>> {
        require!(!instruments.is_empty(), "no bootstrap helpers given");

        let curve = Shared::new_cyclic(|weak: &Weak<PiecewiseZeroInflationCurve<Linear>>| {
            let self_weak: Weak<dyn ZeroInflationTermStructure> = weak.clone();
            let lazy = shared_mut(LazyObject::new(true));
            let observable = lazy.borrow().observable_handle();
            let updater = shared_mut(CurveUpdater {
                lazy: SharedMut::clone(&lazy),
            });
            PiecewiseZeroInflationCurve {
                inflation: InflationTermStructureBase::with_reference_date(
                    reference_date,
                    base_date,
                    frequency,
                    Some(day_counter),
                    None,
                ),
                instruments,
                interpolator: Linear,
                data: RefCell::new(CurveData::new()),
                lazy,
                observable,
                updater,
                bootstrap: IterativeBootstrap::new(),
                accuracy: 1.0e-14,
                self_weak,
            }
        });

        let observer = SharedMut::clone(&curve.updater) as SharedMut<dyn Observer>;
        for helper in &curve.instruments {
            helper.observable().register_observer(&observer);
        }
        Ok(curve)
    }
}

impl<I: Interpolator + 'static> PiecewiseZeroInflationCurve<I> {
    /// Runs the bootstrap if the cache is stale, caching the result
    /// (`performCalculations`, `:165-171`). The lazy core is not borrowed while
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

    /// The node times, after bootstrapping (`:143-147`). The first is negative,
    /// the base date preceding the reference date.
    pub fn times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.data.borrow().times().to_vec())
    }

    /// The node dates, after bootstrapping (`:149-153`). The first is the base
    /// date.
    pub fn dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.data.borrow().dates().to_vec())
    }

    /// The node zero-coupon inflation rates, after bootstrapping (`:155-159`).
    pub fn data(&self) -> QlResult<Vec<Real>> {
        self.calculate()?;
        Ok(self.data.borrow().data().to_vec())
    }

    /// The (date, zero rate) nodes, after bootstrapping (`:161-166`).
    pub fn nodes(&self) -> QlResult<Vec<(Date, Real)>> {
        self.calculate()?;
        Ok(self.data.borrow().nodes())
    }

    /// Registers a downstream observer of the curve's notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }
}

impl<I: Interpolator> AsObservable for PiecewiseZeroInflationCurve<I> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<I: Interpolator + 'static> TermStructure for PiecewiseZeroInflationCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.inflation.term_structure_base()
    }

    fn max_date(&self) -> Date {
        // Trigger the bootstrap so the maximum reflects the solved curve
        // (`:135-139`); a bootstrap failure is surfaced by the reads, so fall
        // back here.
        let _ = self.calculate();
        self.data
            .borrow()
            .max_date()
            .or_else(|| self.inflation.term_structure_base().reference_date().ok())
            .unwrap_or_else(Date::null)
    }
}

impl<I: Interpolator + 'static> InflationTermStructure for PiecewiseZeroInflationCurve<I> {
    fn inflation_base(&self) -> &InflationTermStructureBase {
        &self.inflation
    }
}

impl<I: Interpolator + 'static> ZeroInflationTermStructure for PiecewiseZeroInflationCurve<I> {
    fn zero_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.calculate()?;
        let data = self.data.borrow();
        data.interpolation()?.value(t)
    }
}

impl<I: Interpolator + 'static> PiecewiseCurve for PiecewiseZeroInflationCurve<I> {
    type Traits = ZeroInflationTraits;
    type Interp = I;
    type TS = dyn ZeroInflationTermStructure;
    type Helper = dyn ZeroInflationHelper;

    fn instruments(&self) -> &[Shared<dyn ZeroInflationHelper>] {
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
    /// date (`ZeroInflationTraits::initialDate`, `inflationtraits.hpp:46-48`).
    fn initial_date(&self) -> QlResult<Date> {
        Ok(InflationTermStructure::base_date(self))
    }

    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        TermStructure::time_from_reference(self, date)
    }

    fn term_structure_shared(&self) -> QlResult<Shared<dyn ZeroInflationTermStructure>> {
        match self.self_weak.upgrade() {
            Some(curve) => Ok(curve),
            None => crate::fail!("curve dropped before bootstrap"),
        }
    }
}

#[cfg(test)]
mod tests {
    //! The full oracle - `inflation.cpp`'s `testZeroTermStructure`, which
    //! reprices fourteen quoted swaps off the bootstrapped curve - is the
    //! sibling module below. What is checked here is what the curve decides on
    //! its own: that the bootstrap runs lazily, that it lays its first node on
    //! the base date at a negative time rather than on the reference date, and
    //! that every helper reprices its own quote off the result.

    use super::*;
    use crate::handle::Handle;
    use crate::indexes::Index;
    use crate::indexes::inflation::UkRpi;
    use crate::indexes::inflationindex::{CpiInterpolationType, ZeroInflationIndex};
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::inflation::inflationhelpers::ZeroCouponInflationSwapHelper;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::unitedkingdom::{Market, UnitedKingdom};
    use crate::time::date::Month::{April, August, July, June, May};
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    const QUOTES: [Real; 3] = [0.029, 0.030, 0.031];
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

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today());
        settings
    }

    /// UK RPI with the four figures the fixture needs: the swaps' own base
    /// observation (May 2007, three months before the evaluation date) and the
    /// run up to the curve's base date, which every forecast compounds off.
    fn an_index(settings: &Shared<Settings<Date>>) -> Shared<ZeroInflationIndex> {
        let index = shared(UkRpi::new(Shared::clone(settings)));
        for (date, fixing) in [
            (Date::new(1, April, 2007), 204.4),
            (Date::new(1, May, 2007), 205.4),
            (Date::new(1, June, 2007), 206.2),
            (base_date(), 207.3),
        ] {
            index.add_fixing(date, fixing).expect("a published figure");
        }
        index
    }

    fn helpers(
        settings: &Shared<Settings<Date>>,
        index: &Shared<ZeroInflationIndex>,
    ) -> Vec<Shared<dyn ZeroInflationHelper>> {
        QUOTES
            .iter()
            .zip(MATURITY_YEARS)
            .map(|(quote, years)| {
                ZeroCouponInflationSwapHelper::new(
                    Handle::new(shared(SimpleQuote::new(Some(*quote))) as Shared<dyn Quote>),
                    Period::new(3, TimeUnit::Months),
                    today() + Period::new(years, TimeUnit::Years),
                    UnitedKingdom::new(Market::Settlement),
                    BusinessDayConvention::ModifiedFollowing,
                    day_counter(),
                    index,
                    CpiInterpolationType::Flat,
                    Shared::clone(settings),
                )
                .expect("a three-month lag covers UK RPI's availability")
                    as Shared<dyn ZeroInflationHelper>
            })
            .collect()
    }

    struct Fixture {
        _settings: Shared<Settings<Date>>,
        helpers: Vec<Shared<dyn ZeroInflationHelper>>,
        curve: Shared<PiecewiseZeroInflationCurve<Linear>>,
    }

    fn a_curve() -> Fixture {
        let settings = settings_today();
        let index = an_index(&settings);
        assert_eq!(
            index.last_fixing_date().unwrap(),
            base_date(),
            "the curve's base date is the index's last published period"
        );
        let helpers = helpers(&settings, &index);
        let curve = PiecewiseZeroInflationCurve::new(
            today(),
            base_date(),
            Frequency::Monthly,
            day_counter(),
            helpers.clone(),
        )
        .unwrap();
        Fixture {
            _settings: settings,
            helpers,
            curve,
        }
    }

    /// The T3 seam: node zero sits on the base date, at the negative time that
    /// separates it from the reference date. A curve that took the driver's
    /// default `initial_date` would put it on 13 August 2007 at time zero.
    ///
    /// The time is pinned exactly rather than by sign, so it discriminates the
    /// day counter too: `Thirty360(BondBasis)` from 13 August 2007 back to 1
    /// July 2007 is `(30 * (7 - 8) + (1 - 13)) / 360 = -42/360`.
    #[test]
    fn the_first_node_is_the_base_date_at_a_negative_time() {
        let fixture = a_curve();
        let (helpers, curve) = (&fixture.helpers, &fixture.curve);

        assert_eq!(curve.dates().unwrap()[0], base_date());
        assert_eq!(curve.times().unwrap()[0], -42.0 / 360.0);
        assert_eq!(
            curve.times().unwrap()[0],
            TermStructure::time_from_reference(curve.as_ref(), base_date()).unwrap()
        );
        assert_eq!(
            curve.dates().unwrap().len(),
            helpers.len() + 1,
            "one node per helper, plus the base-date node"
        );
        assert_eq!(curve.nodes().unwrap()[0].0, base_date());
    }

    /// Every helper reprices its own quoted rate off the bootstrapped curve.
    #[test]
    fn the_bootstrapped_curve_reproduces_the_quoted_swap_rates() {
        let fixture = a_curve();
        let (helpers, curve) = (&fixture.helpers, &fixture.curve);
        curve.calculate().unwrap();

        for (helper, quote) in helpers.iter().zip(QUOTES) {
            let error = helper.quote_error().unwrap();
            assert!(
                error.abs() < 1.0e-12,
                "quote error {error} on the {quote} helper"
            );
        }
    }

    /// The pillars are the helpers' observed fixing periods, and the node rates
    /// are plausible inflation rates rather than the traits' `0.02` seed.
    #[test]
    fn the_pillars_are_the_helpers_fixing_periods() {
        let fixture = a_curve();
        let (helpers, curve) = (&fixture.helpers, &fixture.curve);
        let dates = curve.dates().unwrap();

        for (i, helper) in helpers.iter().enumerate() {
            assert_eq!(dates[i + 1], helper.pillar_date());
        }
        assert_eq!(dates[1], Date::new(1, May, 2008));
        assert_eq!(curve.max_date(), *dates.last().unwrap());

        for rate in curve.data().unwrap() {
            assert!((0.01..0.05).contains(&rate), "node rate {rate}");
        }
    }

    /// Construction lays down no nodes and runs no solver; the first read
    /// bootstraps; a quote move invalidates the cache and the next read
    /// re-bootstraps to a higher curve (the C++ `LazyObject` contract, and the
    /// [`CurveUpdater`] registration that carries the quote's notification).
    #[test]
    fn the_bootstrap_is_lazy_and_reruns_on_a_quote_change() {
        let settings = settings_today();
        let index = an_index(&settings);
        let quote = shared(SimpleQuote::new(Some(0.029)));
        let helper = ZeroCouponInflationSwapHelper::new(
            Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
            Period::new(3, TimeUnit::Months),
            today() + Period::new(5, TimeUnit::Years),
            UnitedKingdom::new(Market::Settlement),
            BusinessDayConvention::ModifiedFollowing,
            day_counter(),
            &index,
            CpiInterpolationType::Flat,
            Shared::clone(&settings),
        )
        .unwrap();
        let curve = PiecewiseZeroInflationCurve::new(
            today(),
            base_date(),
            Frequency::Monthly,
            day_counter(),
            vec![Shared::clone(&helper) as Shared<dyn ZeroInflationHelper>],
        )
        .unwrap();

        assert!(!curve.lazy.borrow().is_calculated());
        let first = curve.data().unwrap()[1];
        assert!(curve.lazy.borrow().is_calculated());
        assert!((0.01..0.05).contains(&first), "solved {first}");

        quote.set_value(Some(0.04));
        assert!(!curve.lazy.borrow().is_calculated());
        let second = curve.data().unwrap()[1];
        assert!(
            second > first,
            "a higher quoted rate must lift the curve: {second} vs {first}"
        );
    }

    #[test]
    fn an_empty_helper_set_is_rejected() {
        let built = PiecewiseZeroInflationCurve::new(
            today(),
            base_date(),
            Frequency::Monthly,
            day_counter(),
            Vec::new(),
        );
        let err = match built {
            Ok(_) => panic!("expected a construction error"),
            Err(err) => err,
        };
        assert!(err.message().contains("no bootstrap helpers"));
    }
}

#[cfg(test)]
mod zero_term_structure_oracle {
    //! `test-suite/inflation.cpp` `testZeroTermStructure` (`:320-463`): the UK
    //! RPI fixture of August 2007, where fourteen quoted zero-coupon inflation
    //! swaps bootstrap the curve and are then repriced off it - standalone
    //! contracts on the *real* 5 % nominal curve, not the helpers' own
    //! zero-strike swaps on their flat 0 % one - and must come back worth
    //! nothing.
    //!
    //! That reprice-to-zero is the milestone assertion of EPIC Inflation
    //! (#705). It is an absolute check rather than a self-consistent round
    //! trip: it discriminates the bootstrap's convergence, the base-date node
    //! placement, the fixing-period quantization and the forecast formula all
    //! at once, at the C++ tolerance of 1e-7.
    //!
    //! Phase 3 (`:465-506`), which adds a seasonality correction and reprices
    //! again, is **omitted**: seasonality is a documented deferral of this port
    //! (see the module divergences), and the Rust curve takes no seasonality
    //! argument, so phases 1 and 2 run exactly the numbers C++ runs before it.

    use super::*;
    use crate::handle::{Handle, RelinkableHandle};
    use crate::indexes::Index;
    use crate::indexes::inflation::UkRpi;
    use crate::indexes::inflationindex::{
        CpiInterpolationType, ZeroInflationIndex, inflation_period,
    };
    use crate::instrument::Instrument;
    use crate::instruments::{SwapType, ZeroCouponInflationSwap};
    use crate::interestrate::Compounding;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::DiscountingSwapEngine;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::termstructures::inflation::inflationhelpers::ZeroCouponInflationSwapHelper;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::unitedkingdom::{Market, UnitedKingdom};
    use crate::time::date::Month::{August, January, July, May};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::period::Period;
    use crate::time::schedule::MakeSchedule;
    use crate::time::timeunit::TimeUnit;

    /// The C++ tolerance both phases run at (`:402`).
    const EPS: Real = 1.0e-7;
    /// The shift the fixed-leg BPS is checked against (`:403`).
    const BASIS_POINT: Real = 1.0e-4;
    const NOMINAL: Real = 1_000_000.0;

    /// The UK RPI figures published monthly from January 2005 to July 2007
    /// (`:342-348`).
    const FIX_DATA: [Real; 31] = [
        189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1,
        193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6,
        203.1, 204.4, 205.4, 206.2, 207.3,
    ];

    /// The quoted zero-coupon inflation swap rates, in per cent (`:358-373`).
    fn zc_data() -> Vec<(Date, Real)> {
        vec![
            (Date::new(13, August, 2008), 2.93),
            (Date::new(13, August, 2009), 2.95),
            (Date::new(13, August, 2010), 2.965),
            (Date::new(15, August, 2011), 2.98),
            (Date::new(13, August, 2012), 3.0),
            (Date::new(13, August, 2014), 3.06),
            (Date::new(13, August, 2017), 3.175),
            (Date::new(13, August, 2019), 3.243),
            (Date::new(15, August, 2022), 3.293),
            (Date::new(14, August, 2027), 3.338),
            (Date::new(13, August, 2032), 3.348),
            (Date::new(15, August, 2037), 3.348),
            (Date::new(13, August, 2047), 3.308),
            (Date::new(13, August, 2057), 3.228),
        ]
    }

    fn calendar() -> Calendar {
        UnitedKingdom::new(Market::Settlement)
    }

    fn evaluation_date() -> Date {
        Date::new(13, August, 2007)
    }

    fn day_counter() -> DayCounter {
        Thirty360::with_convention(Convention::BondBasis)
    }

    fn observation_lag() -> Period {
        Period::new(3, TimeUnit::Months)
    }

    struct Fixture {
        settings: Shared<Settings<Date>>,
        index: Shared<ZeroInflationIndex>,
        nominal_ts: Handle<dyn YieldTermStructure>,
        curve: Shared<PiecewiseZeroInflationCurve<Linear>>,
        first_helper: Shared<ZeroCouponInflationSwapHelper>,
        /// Kept alive for the length of the fixture: it is the link the index
        /// forecasts through once the curve is bootstrapped.
        _hz: RelinkableHandle<dyn ZeroInflationTermStructure>,
    }

    impl Fixture {
        /// A standalone quoted swap on the *real* nominal curve (`:406-417`).
        /// The helper's own swap is struck at zero on a flat 0 % curve and would
        /// not answer this.
        fn a_swap(&self, maturity: Date, fixed_rate: Rate) -> ZeroCouponInflationSwap {
            let mut swap = ZeroCouponInflationSwap::new(
                SwapType::Payer,
                NOMINAL,
                evaluation_date(),
                maturity,
                calendar(),
                BusinessDayConvention::ModifiedFollowing,
                day_counter(),
                fixed_rate,
                Shared::clone(&self.index),
                observation_lag(),
                CpiInterpolationType::Flat,
                None,
                None,
                Shared::clone(&self.settings),
            )
            .expect("a three-month lag covers UK RPI's availability");
            let engine = DiscountingSwapEngine::new(
                self.nominal_ts.clone(),
                None,
                None,
                None,
                Shared::clone(&self.settings),
            );
            swap.base_mut()
                .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);
            swap
        }
    }

    /// The whole C++ wiring (`:322-397`). The index is built on an empty
    /// relinkable handle and only relinked to the curve once that is
    /// constructed, as C++ does: the helpers forecast through copies of the
    /// index linked to their own handles, so this one matters only to the
    /// standalone swaps and to phase 2.
    fn a_fixture() -> Fixture {
        assert_eq!(
            calendar().adjust(evaluation_date(), BusinessDayConvention::ModifiedFollowing),
            evaluation_date(),
            "13 August 2007 is a UK business day, so C++'s adjust is the identity"
        );
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(evaluation_date());

        let hz: RelinkableHandle<dyn ZeroInflationTermStructure> = RelinkableHandle::empty();
        let index = shared(UkRpi::new(Shared::clone(&settings)).with_term_structure(hz.handle()));
        // The C++ fixing schedule is monthly from 1 January 2005 and is never
        // adjusted, the first of the month being what a monthly index is filed
        // under.
        let first_fixing_date = Date::new(1, January, 2005);
        for (i, fixing) in FIX_DATA.iter().enumerate() {
            let date = first_fixing_date + Period::new(i as i32, TimeUnit::Months);
            index.add_fixing(date, *fixing).expect("a published figure");
        }

        let nominal_ts: Handle<dyn YieldTermStructure> =
            Handle::new(shared(FlatForward::with_rate(
                evaluation_date(),
                0.05,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>);

        let built: Vec<Shared<ZeroCouponInflationSwapHelper>> = zc_data()
            .iter()
            .map(|(maturity, rate)| {
                ZeroCouponInflationSwapHelper::new(
                    Handle::new(shared(SimpleQuote::new(Some(rate / 100.0))) as Shared<dyn Quote>),
                    observation_lag(),
                    *maturity,
                    calendar(),
                    BusinessDayConvention::ModifiedFollowing,
                    day_counter(),
                    &index,
                    CpiInterpolationType::Flat,
                    Shared::clone(&settings),
                )
                .expect("a three-month lag covers UK RPI's availability")
            })
            .collect();
        let helpers: Vec<Shared<dyn ZeroInflationHelper>> = built
            .iter()
            .map(|helper| Shared::clone(helper) as Shared<dyn ZeroInflationHelper>)
            .collect();

        let base_date = index.last_fixing_date().expect("the fixings are on record");
        assert_eq!(
            base_date,
            Date::new(1, July, 2007),
            "the base date is the period of the last published figure"
        );
        let curve = PiecewiseZeroInflationCurve::new(
            evaluation_date(),
            base_date,
            Frequency::Monthly,
            day_counter(),
            helpers,
        )
        .unwrap();
        hz.link_to(Shared::clone(&curve) as Shared<dyn ZeroInflationTermStructure>);

        Fixture {
            settings,
            index,
            nominal_ts,
            first_helper: Shared::clone(&built[0]),
            curve,
            _hz: hz,
        }
    }

    /// Phase 1 (`:400-434`), the milestone: every quoted swap, rebuilt
    /// standalone and discounted on the 5 % nominal curve, prices to zero off
    /// the bootstrapped inflation curve; and the analytic fixed-leg BPS matches
    /// a repriced one-basis-point bump of the same contract.
    ///
    /// The two PINs that come first are what make a failure readable. The
    /// helper's observation date is checked before the bootstrap (`:385`), and
    /// the base-date node placement is checked through
    /// [`dates`](PiecewiseZeroInflationCurve::dates), which propagates a
    /// bootstrap error - a range-checked query would instead report the
    /// evaluation date as the maximum and hide it.
    #[test]
    fn the_bootstrapped_curve_reprices_the_quoted_swaps_to_zero() {
        let fixture = a_fixture();
        assert_eq!(
            fixture
                .first_helper
                .swap()
                .as_ref()
                .expect("the helper's swap builds")
                .inflation_cash_flow()
                .fixing_date(),
            Date::new(13, May, 2008)
        );

        let curve = &fixture.curve;
        assert_eq!(curve.dates().unwrap()[0], curve.base_date());
        assert!(
            curve.times().unwrap()[0] < 0.0,
            "the base node precedes the reference date: {}",
            curve.times().unwrap()[0]
        );

        let (mut worst_npv, mut worst_bps) = (0.0_f64, 0.0_f64);
        for (maturity, rate) in zc_data() {
            let mut swap = fixture.a_swap(maturity, rate / 100.0);
            worst_npv = worst_npv.max(swap.npv().unwrap().abs());

            let mut bumped = fixture.a_swap(maturity, rate / 100.0 + BASIS_POINT);
            let expected = bumped.fixed_leg_npv().unwrap() - swap.fixed_leg_npv().unwrap();
            worst_bps = worst_bps.max((swap.fixed_leg_bps().unwrap() - expected).abs());
        }
        println!("worst |NPV| {worst_npv:e}, worst fixed-leg BPS error {worst_bps:e}");
        assert!(worst_npv < EPS, "worst |NPV| {worst_npv}");
        assert!(worst_bps < EPS, "worst fixed-leg BPS error {worst_bps}");
    }

    /// Phase 2 (`:437-463`): the index, forecasting off the bootstrapped curve,
    /// reproduces the curve's own zero rate compounded off the base fixing at
    /// every monthly date from the reference date to a month short of the
    /// maximum.
    ///
    /// The `t <= 0` branch is the C++ one for a date still inside history; this
    /// fixture never takes it, the schedule starting after the base period.
    #[test]
    fn the_index_forecasts_off_the_bootstrapped_curve() {
        let fixture = a_fixture();
        let curve = &fixture.curve;
        let schedule = MakeSchedule::new()
            .from(TermStructure::reference_date(curve.as_ref()).unwrap())
            .to(curve.max_date() - Period::new(1, TimeUnit::Months))
            .with_tenor(Period::new(1, TimeUnit::Months))
            .with_calendar(calendar())
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .build();

        let base_date = curve.base_date();
        let base_fixing = fixture.index.fixing(base_date, false).unwrap();
        let curve_day_counter = curve.require_day_counter().unwrap();

        let mut worst = 0.0_f64;
        for &date in schedule.dates() {
            let z = curve.zero_rate_date(date, false).unwrap();
            let period_start = inflation_period(date, Frequency::Monthly).unwrap().0;
            let t = curve_day_counter.year_fraction(base_date, period_start);
            let calc = if t <= 0.0 {
                fixture.index.fixing(date, false).unwrap()
            } else {
                base_fixing * (1.0 + z).powf(t)
            };
            let forecast = fixture.index.fixing(date, true).unwrap();
            worst = worst.max((calc - forecast).abs());
        }
        println!(
            "worst forecast error {worst:e} over {} dates",
            schedule.len()
        );
        assert!(worst < EPS, "worst forecast error {worst}");
    }
}
