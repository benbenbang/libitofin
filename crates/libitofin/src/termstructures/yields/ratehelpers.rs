//! Rate helpers for the yield-curve bootstrap.
//!
//! Port of `ql/termstructures/yield/ratehelpers.{hpp,cpp}` plus the overnight
//! helper of `ql/termstructures/yield/oisratehelper.{hpp,cpp}`. This module holds
//! [`DepositRateHelper`], the short end of the curve, [`SwapRateHelper`], the
//! long end over vanilla swaps, [`OISRateHelper`], the long end over
//! overnight-indexed swaps, [`FuturesRateHelper`], the short-to-mid end over
//! exchange-traded IMM/ASX futures, and [`FraRateHelper`], the mid end over
//! forward-rate agreements. `BMASwapRateHelper` is deferred to a later ticket
//! (#343); the `SwapIndex`-based and explicit-start/end-date `SwapRateHelper`
//! constructors are deferred with the swap-index port. The `OISRateHelper`
//! explicit-start/end-date constructor is deferred likewise.
//!
//! [`FraRateHelper`] defers three `FraRateHelper` constructor paths visibly: the
//! `Pillar::CustomDate` variants (that enum arm is not ported, see [`Pillar`]),
//! the from-scratch constructors that build a synthetic `"no-fix"` [`IborIndex`]
//! (`ratehelpers.cpp:263,293`), and the IMM-offset constructors
//! (`ratehelpers.cpp:322`, with the `nthImmDate` helper). None sit on the
//! bootstrap oracle path. Unlike C++, which registers the helper with its cloned
//! index (`registerWith(iborIndex_)`) and so must unregister that index from the
//! forwarding handle to keep relink notifications out of the bootstrap, the Rust
//! helper never observes the cloned index (matching [`DepositRateHelper`]), so
//! the relink-notification loop is already broken and no unregister is needed.

use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Weak;

use crate::cashflows::RateAveraging;
use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::OvernightIndex;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::{
    FuturesType, MakeOis, MakeVanillaSwap, OvernightIndexedSwap, VanillaSwap,
};
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::termstructures::bootstraphelper::{
    BootstrapHelperBase, RateHelper, RelativeDateRateHelper,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::time::{asx, imm};
use crate::types::{Integer, Natural, Real};

/// Bootstrap helper over a deposit rate (`DepositRateHelper`).
///
/// A deposit borrows at the quoted rate from spot to spot-plus-tenor;
/// [`implied_quote`](RateHelper::implied_quote) re-derives that rate from the
/// bootstrapping curve's discount factors between the value and maturity dates.
///
/// The load-bearing mechanism is the cloned index: the constructor re-curves
/// the supplied index onto the helper's *own* [`RelinkableHandle`] with
/// [`IborIndex::clone_with`] (`ratehelpers.cpp:206`), so the helper forecasts
/// off the curve it is being bootstrapped against rather than whatever curve
/// the index was handed. [`set_term_structure`](RateHelper::set_term_structure)
/// weak-links that handle to the bootstrapping curve, non-owning and unobserved
/// (the `null_deleter`/`observer = false` of `ratehelpers.cpp:217`).
pub struct DepositRateHelper {
    base: BootstrapHelperBase,
    index: IborIndex,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
    fixing_date: Cell<Date>,
}

impl DepositRateHelper {
    /// A deposit helper fitting `quote`, an explicit market-rate handle, with
    /// its schedule taken from `index` (the C++ `DepositRateHelper(rate, i)`
    /// with `rate` a `Handle<Quote>`, `ratehelpers.cpp:195`).
    pub fn new(quote: Handle<dyn Quote>, index: &IborIndex) -> Shared<DepositRateHelper> {
        Self::build(quote, index)
    }

    /// A deposit helper fitting a fixed `rate`, wrapped in a [`SimpleQuote`]
    /// (the `rate`-as-`Rate` arm of the same C++ variant constructor).
    pub fn from_rate(rate: Real, index: &IborIndex) -> Shared<DepositRateHelper> {
        let quote = Handle::new(shared(SimpleQuote::new(rate)) as Shared<dyn Quote>);
        Self::build(quote, index)
    }

    fn build(quote: Handle<dyn Quote>, source_index: &IborIndex) -> Shared<DepositRateHelper> {
        let settings = source_index.base().settings().clone();
        Shared::new_cyclic(|weak: &Weak<DepositRateHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                }
            });
            let term_structure_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
            let index = source_index.clone_with(term_structure_handle.handle());
            let base = BootstrapHelperBase::new_relative(quote, settings, true, on_eval_change);
            let helper = DepositRateHelper {
                base,
                index,
                term_structure_handle,
                fixing_date: Cell::new(Date::null()),
            };
            helper.initialize_dates();
            helper
        })
    }
}

impl AsObservable for DepositRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for DepositRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    /// The deposit rate implied by the current curve.
    ///
    /// The forecast flag is forced true (`iborIndex_->fixing(fixingDate_, true)`,
    /// `ratehelpers.cpp:213`): the helper prices off the curve, never off a
    /// stored fixing.
    fn implied_quote(&self) -> QlResult<Real> {
        self.base.term_structure()?;
        self.index.fixing(self.fixing_date.get(), true)
    }

    /// Weak-links the helper's own pricing handle to the bootstrapping curve,
    /// then records the curve on the base - both non-owning and unobserved
    /// (`ratehelpers.cpp:216`).
    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        self.base.set_term_structure(term_structure);
    }
}

impl RelativeDateRateHelper for DepositRateHelper {
    /// Rebuilds the schedule off the current evaluation date
    /// (`initializeDates`, `ratehelpers.cpp:228`): the reference date is the
    /// evaluation date rolled to a business day, the earliest (value) date is
    /// spot from there, the fixing date the value date rolled back, and the
    /// maturity the value date advanced by the tenor. Pillar, latest and
    /// latest-relevant dates all equal the maturity.
    ///
    /// The value- and maturity-date arithmetic is calendar rolling on an
    /// already-adjusted business day and so cannot fail; the `expect` documents
    /// that invariant.
    fn initialize_dates(&self) {
        let evaluation_date = self
            .base
            .evaluation_date()
            .expect("a relative-date helper always tracks an evaluation date");
        let reference_date = self
            .index
            .fixing_calendar()
            .adjust(evaluation_date, BusinessDayConvention::Following);
        let earliest = self
            .index
            .value_date(reference_date)
            .expect("value date of an adjusted business day is valid");
        self.fixing_date.set(self.index.fixing_date(earliest));
        let maturity = self
            .index
            .maturity_date(earliest)
            .expect("maturity date of a value date is valid");

        self.base.set_earliest_date(earliest);
        self.base.set_maturity_date(maturity);
        self.base.set_pillar_date(maturity);
        self.base.set_latest_date(maturity);
        self.base.set_latest_relevant_date(maturity);
    }
}

/// Validates that `date` is a legal settlement date for the futures convention
/// (`CheckDate`, `ratehelpers.cpp:46`): IMM and ASX require a valid cycle date,
/// while [`FuturesType::Custom`] imposes no constraint.
fn check_futures_date(date: Date, futures_type: FuturesType) -> QlResult<()> {
    match futures_type {
        FuturesType::Imm => {
            crate::require!(
                imm::is_imm_date(date, false),
                "{date} is not a valid IMM date"
            )
        }
        FuturesType::Asx => {
            crate::require!(
                asx::is_asx_date(date, false),
                "{date} is not a valid ASX date"
            )
        }
        FuturesType::Custom => {}
    }
    Ok(())
}

/// Resolves the maturity of an explicit-date futures helper (the C++
/// `determineMaturityDate` lambda, `ratehelpers.cpp:100`): with no end date,
/// advance three futures periods off `next_date`; with one, take it after
/// checking it is past the start.
fn determine_maturity(
    start: Date,
    end: Option<Date>,
    next_date: impl Fn(Date) -> Date,
) -> QlResult<Date> {
    match end {
        None => Ok(next_date(next_date(next_date(start)))),
        Some(end) => {
            crate::require!(
                end > start,
                "end date ({end}) must be greater than start date ({start})"
            );
            Ok(end)
        }
    }
}

/// Bootstrap helper over an exchange-traded interest-rate future
/// (`FuturesRateHelper`).
///
/// The helper fits the quoted futures price `P` (e.g. `96.0` for a 4% implied
/// rate) at a fixed IMM/ASX window. Unlike the deposit, swap and OIS helpers it
/// derives from the **plain** [`RateHelper`], not [`RelativeDateRateHelper`]:
/// the earliest and maturity dates are absolute, computed once at construction
/// from the supplied dates, and never rebuilt on an evaluation-date change
/// (there is no `initialize_dates`). It therefore holds the bootstrapping curve
/// directly through [`base`](RateHelper::base) rather than forecasting off a
/// cloned index on its own relinkable handle.
///
/// [`implied_quote`](RateHelper::implied_quote) re-derives the price from the
/// curve (`ratehelpers.cpp:157`): the simple forward over the window is
/// `(discount(earliest)/discount(maturity) - 1)/year_fraction`, and the price is
/// `100 * (1 - (forward + convexity_adjustment))`. The convexity adjustment is a
/// plain [`Handle<Quote>`](Handle) value with no convexity model (the C++
/// `FuturesConvAdjustmentQuote` is not ported); it may be negative, as futures
/// margining can push the futures rate either side of the forward.
pub struct FuturesRateHelper {
    base: BootstrapHelperBase,
    year_fraction: Real,
    conv_adj: Handle<dyn Quote>,
}

impl FuturesRateHelper {
    /// A futures helper over a length-in-months window off `ibor_start_date`
    /// (`ratehelpers.cpp:70`): the maturity is the start advanced
    /// `length_in_months` months on `calendar` under `convention`/`end_of_month`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        price: Handle<dyn Quote>,
        ibor_start_date: Date,
        length_in_months: Natural,
        calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
        day_counter: DayCounter,
        conv_adj: Handle<dyn Quote>,
        futures_type: FuturesType,
    ) -> QlResult<Shared<FuturesRateHelper>> {
        check_futures_date(ibor_start_date, futures_type)?;
        let earliest = ibor_start_date;
        let maturity = calendar.advance(
            ibor_start_date,
            length_in_months as Integer,
            TimeUnit::Months,
            convention,
            end_of_month,
        );
        let year_fraction = day_counter.year_fraction_ref(earliest, maturity, earliest, maturity);
        Ok(Self::assemble(
            price,
            conv_adj,
            earliest,
            maturity,
            year_fraction,
        ))
    }

    /// A futures helper over an explicit window (`ratehelpers.cpp:91`). With
    /// `ibor_end_date` `None` the maturity is three [`imm`]/[`asx`] periods past
    /// the start; with `Some`, that date (which must be past the start for the
    /// IMM/ASX conventions). [`FuturesType::Custom`] requires an explicit end
    /// date - the C++ null-maturity path is an error here.
    pub fn from_end_date(
        price: Handle<dyn Quote>,
        ibor_start_date: Date,
        ibor_end_date: Option<Date>,
        day_counter: DayCounter,
        conv_adj: Handle<dyn Quote>,
        futures_type: FuturesType,
    ) -> QlResult<Shared<FuturesRateHelper>> {
        check_futures_date(ibor_start_date, futures_type)?;
        let maturity = match futures_type {
            FuturesType::Imm => {
                determine_maturity(ibor_start_date, ibor_end_date, |d| imm::next_date(d, false))?
            }
            FuturesType::Asx => {
                determine_maturity(ibor_start_date, ibor_end_date, |d| asx::next_date(d, false))?
            }
            FuturesType::Custom => match ibor_end_date {
                Some(end) => end,
                None => crate::fail!("a Custom futures helper requires an explicit end date"),
            },
        };
        let earliest = ibor_start_date;
        let year_fraction = day_counter.year_fraction_ref(earliest, maturity, earliest, maturity);
        Ok(Self::assemble(
            price,
            conv_adj,
            earliest,
            maturity,
            year_fraction,
        ))
    }

    /// A futures helper whose window follows `index`'s conventions
    /// (`ratehelpers.cpp:139`): the maturity is the start advanced by the index
    /// tenor on the index's fixing calendar under its business-day convention (no
    /// end-of-month), and the year fraction uses the index day counter.
    pub fn from_index(
        price: Handle<dyn Quote>,
        ibor_start_date: Date,
        index: &IborIndex,
        conv_adj: Handle<dyn Quote>,
        futures_type: FuturesType,
    ) -> QlResult<Shared<FuturesRateHelper>> {
        check_futures_date(ibor_start_date, futures_type)?;
        let earliest = ibor_start_date;
        let maturity = index.fixing_calendar().advance_by_period(
            ibor_start_date,
            index.tenor(),
            index.business_day_convention(),
            false,
        );
        let year_fraction = index
            .day_counter()
            .year_fraction_ref(earliest, maturity, earliest, maturity);
        Ok(Self::assemble(
            price,
            conv_adj,
            earliest,
            maturity,
            year_fraction,
        ))
    }

    /// Builds the plain-date base shared by the three constructors: observes the
    /// price (via the base) and the convexity quote (the C++
    /// `registerWith(convAdj_)`), then pins every schedule date to the fixed
    /// window - `pillar = latest = latest_relevant = maturity`.
    fn assemble(
        price: Handle<dyn Quote>,
        conv_adj: Handle<dyn Quote>,
        earliest: Date,
        maturity: Date,
        year_fraction: Real,
    ) -> Shared<FuturesRateHelper> {
        let base = BootstrapHelperBase::new(price);
        conv_adj.register_observer(&base.observer());
        base.set_earliest_date(earliest);
        base.set_maturity_date(maturity);
        base.set_pillar_date(maturity);
        base.set_latest_date(maturity);
        base.set_latest_relevant_date(maturity);
        shared(FuturesRateHelper {
            base,
            year_fraction,
            conv_adj,
        })
    }

    /// The convexity adjustment applied to the forward (`ratehelpers.cpp:168`):
    /// the convexity quote's value, or zero when no quote was supplied.
    pub fn convexity_adjustment(&self) -> QlResult<Real> {
        if self.conv_adj.is_empty() {
            Ok(0.0)
        } else {
            self.conv_adj.current_link()?.value()
        }
    }
}

impl AsObservable for FuturesRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for FuturesRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    /// The futures price implied by the current curve (`ratehelpers.cpp:157`).
    ///
    /// The simple forward over the fixed window comes straight off the curve's
    /// discount factors; the price adds the convexity adjustment to that forward
    /// and quotes `100 * (1 - future_rate)`.
    fn implied_quote(&self) -> QlResult<Real> {
        let term_structure = self.base.term_structure()?;
        let forward = (term_structure.discount_date(self.base.earliest_date(), false)?
            / term_structure.discount_date(self.base.maturity_date(), false)?
            - 1.0)
            / self.year_fraction;
        let future_rate = forward + self.convexity_adjustment()?;
        Ok(100.0 * (1.0 - future_rate))
    }
}

/// The date the curve node a helper fits sits at (`Pillar::Choice`).
///
/// Only the two schedule-derived choices are ported: [`LastRelevantDate`] (the
/// C++ default) and [`MaturityDate`]. `Pillar::CustomDate`, which needs an
/// explicit pillar date threaded through construction plus its bounds check, is
/// deferred to #343 with the constructors that pass one.
///
/// [`LastRelevantDate`]: Pillar::LastRelevantDate
/// [`MaturityDate`]: Pillar::MaturityDate
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pillar {
    /// The instrument's maturity date.
    MaturityDate,
    /// The latest date the instrument needs data at.
    LastRelevantDate,
}

/// Bootstrap helper over a forward-rate-agreement rate (`FraRateHelper`).
///
/// The helper fits the rate of a FRA that starts `period_to_start` after spot
/// and spans the index tenor. Like [`DepositRateHelper`] it clones the supplied
/// index onto its own [`RelinkableHandle`] with [`IborIndex::clone_with`]
/// (`ratehelpers.cpp:315`) so it forecasts off the curve being bootstrapped, and
/// [`set_term_structure`](RateHelper::set_term_structure) weak-links that handle
/// to the bootstrapping curve, non-owning and unobserved.
///
/// It runs in one of two modes (`use_indexed_coupon`, C++ default `true`):
/// **indexed**, where [`implied_quote`](RateHelper::implied_quote) is the index
/// fixing forecast off the curve (`ibor_index.fixing(fixing_date, true)`), the
/// node pinned at `index.maturity_date(earliest)`; and **par**, where the quote
/// is the simple forward `(discount(earliest)/discount(maturity) - 1)/tau` over
/// the raw schedule window with `tau` the index day-count fraction, the node
/// pinned at the maturity (`ratehelpers.cpp:361`).
pub struct FraRateHelper {
    base: BootstrapHelperBase,
    index: IborIndex,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
    period_to_start: Option<Period>,
    use_indexed_coupon: bool,
    pillar: Pillar,
    fixing_date: Cell<Date>,
    spanning_time: Cell<Real>,
}

impl FraRateHelper {
    /// A FRA helper fitting `quote` over the window starting `period_to_start`
    /// after spot and spanning `index`'s tenor (the index-based `Period`
    /// constructor, `ratehelpers.cpp:305`).
    pub fn new(
        quote: Handle<dyn Quote>,
        period_to_start: Period,
        index: &IborIndex,
        use_indexed_coupon: bool,
        pillar: Pillar,
    ) -> Shared<FraRateHelper> {
        Self::build(
            quote,
            Some(period_to_start),
            index,
            use_indexed_coupon,
            pillar,
            None,
        )
    }

    /// A FRA helper fitting a fixed `rate`, wrapped in a [`SimpleQuote`] (the
    /// `Rate` arm of the C++ `variant<Rate, Handle<Quote>>` constructor).
    pub fn from_rate(
        rate: Real,
        period_to_start: Period,
        index: &IborIndex,
        use_indexed_coupon: bool,
        pillar: Pillar,
    ) -> Shared<FraRateHelper> {
        let quote = Handle::new(shared(SimpleQuote::new(rate)) as Shared<dyn Quote>);
        Self::new(quote, period_to_start, index, use_indexed_coupon, pillar)
    }

    /// A FRA helper whose start is `months_to_start` months after spot (the
    /// index-based `Natural monthsToStart` constructor, `ratehelpers.cpp:271`,
    /// which delegates to the `Period` form via `months_to_start * Months`).
    pub fn from_months(
        quote: Handle<dyn Quote>,
        months_to_start: Natural,
        index: &IborIndex,
        use_indexed_coupon: bool,
        pillar: Pillar,
    ) -> Shared<FraRateHelper> {
        Self::new(
            quote,
            Period::new(months_to_start as Integer, TimeUnit::Months),
            index,
            use_indexed_coupon,
            pillar,
        )
    }

    /// A FRA helper over an explicit `[start_date, end_date]` window (the
    /// non-relative date constructor, `ratehelpers.cpp:341`). Its schedule is
    /// fixed at construction and, mirroring the C++ `RelativeDateRateHelper(rate,
    /// false)`, does not shift when the evaluation date moves.
    pub fn from_dates(
        quote: Handle<dyn Quote>,
        start_date: Date,
        end_date: Date,
        index: &IborIndex,
        use_indexed_coupon: bool,
        pillar: Pillar,
    ) -> Shared<FraRateHelper> {
        Self::build(
            quote,
            None,
            index,
            use_indexed_coupon,
            pillar,
            Some((start_date, end_date)),
        )
    }

    fn build(
        quote: Handle<dyn Quote>,
        period_to_start: Option<Period>,
        source_index: &IborIndex,
        use_indexed_coupon: bool,
        pillar: Pillar,
        explicit_dates: Option<(Date, Date)>,
    ) -> Shared<FraRateHelper> {
        let settings = source_index.base().settings().clone();
        let update_dates = explicit_dates.is_none();
        Shared::new_cyclic(|weak: &Weak<FraRateHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                }
            });
            let term_structure_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
            let index = source_index.clone_with(term_structure_handle.handle());
            let base =
                BootstrapHelperBase::new_relative(quote, settings, update_dates, on_eval_change);
            let helper = FraRateHelper {
                base,
                index,
                term_structure_handle,
                period_to_start,
                use_indexed_coupon,
                pillar,
                fixing_date: Cell::new(Date::null()),
                spanning_time: Cell::new(0.0),
            };
            if let Some((start, end)) = explicit_dates {
                helper.base.set_earliest_date(start);
                helper.base.set_maturity_date(end);
            }
            helper.initialize_dates();
            helper
        })
    }
}

impl AsObservable for FraRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for FraRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    /// The FRA rate implied by the current curve (`ratehelpers.cpp:361`).
    ///
    /// In indexed mode this is the index fixing forecast off the curve, forced
    /// on (`ibor_index.fixing(fixing_date, true)`); in par mode it is the simple
    /// forward `(discount(earliest)/discount(maturity) - 1)/spanning_time` read
    /// straight from the curve's discount factors.
    fn implied_quote(&self) -> QlResult<Real> {
        let term_structure = self.base.term_structure()?;
        if self.use_indexed_coupon {
            self.index.fixing(self.fixing_date.get(), true)
        } else {
            let discount_earliest =
                term_structure.discount_date(self.base.earliest_date(), false)?;
            let discount_maturity =
                term_structure.discount_date(self.base.maturity_date(), false)?;
            Ok((discount_earliest / discount_maturity - 1.0) / self.spanning_time.get())
        }
    }

    /// Weak-links the helper's own pricing handle to the bootstrapping curve,
    /// then records the curve on the base - both non-owning and unobserved
    /// (`ratehelpers.cpp:372`, `observer = false`).
    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        self.base.set_term_structure(term_structure);
    }
}

impl RelativeDateRateHelper for FraRateHelper {
    /// Rebuilds the schedule off the current evaluation date (`initializeDates`,
    /// `ratehelpers.cpp:393`).
    ///
    /// When the helper tracks the evaluation date (the relative constructors),
    /// the earliest date is `period_to_start` past spot and the maturity is the
    /// combined `period_to_start + tenor` past spot - both advanced from spot,
    /// not chained, so a business-day roll on the intermediate date cannot leak
    /// into the maturity. The explicit-date constructor sets those two dates at
    /// construction and skips this block. Either way the latest-relevant date,
    /// pillar, latest date and fixing date follow: indexed mode pins the node at
    /// `index.maturity_date(earliest)`, par mode at the maturity and caches the
    /// spanning year fraction.
    fn initialize_dates(&self) {
        if self.base.update_dates() {
            let evaluation_date = self
                .base
                .evaluation_date()
                .expect("a relative-date helper always tracks an evaluation date");
            let calendar = self.index.fixing_calendar();
            let reference = calendar.adjust(evaluation_date, BusinessDayConvention::Following);
            let spot = self
                .index
                .value_date(reference)
                .expect("spot date of an adjusted business day is valid");
            let period_to_start = self
                .period_to_start
                .expect("a relative-date FRA helper carries a period to start");
            let convention = self.index.business_day_convention();
            let end_of_month = self.index.end_of_month();
            let earliest =
                calendar.advance_by_period(spot, period_to_start, convention, end_of_month);
            let maturity = calendar.advance_by_period(
                spot,
                period_to_start + self.index.tenor(),
                convention,
                end_of_month,
            );
            self.base.set_earliest_date(earliest);
            self.base.set_maturity_date(maturity);
        }

        let earliest = self.base.earliest_date();
        let maturity = self.base.maturity_date();
        let latest_relevant = if self.use_indexed_coupon {
            self.index
                .maturity_date(earliest)
                .expect("maturity date of a value date is valid")
        } else {
            self.spanning_time
                .set(self.index.day_counter().year_fraction(earliest, maturity));
            maturity
        };
        self.base.set_latest_relevant_date(latest_relevant);
        let pillar = match self.pillar {
            Pillar::MaturityDate => maturity,
            Pillar::LastRelevantDate => latest_relevant,
        };
        self.base.set_pillar_date(pillar);
        self.base.set_latest_date(pillar);
        self.fixing_date.set(self.index.fixing_date(earliest));
    }
}

/// Bootstrap helper over a par swap rate (`SwapRateHelper`).
///
/// The helper fits the fixed rate at which a spot-starting vanilla swap of the
/// quoted tenor is worth par on the bootstrapping curve. Its own swap is built
/// through [`MakeVanillaSwap`] (`ratehelpers.cpp:557`) off a cloned index and a
/// relinkable discounting handle, so the helper prices against the curve it is
/// being bootstrapped against.
///
/// [`implied_quote`](RateHelper::implied_quote) is **not** the swap's fair rate:
/// it is the par-rate reconstruction of `ratehelpers.cpp:633-646`, over the
/// floating- and fixed-leg NPV and BPS, carrying the spread term that a bare
/// `fair_rate()` would drop. Because the helper deliberately does not observe
/// the curve (its pricing handles are weak-linked, unobserved), the swap's
/// cached results go stale when the bootstrap moves the curve; the C++
/// `swap_->deepUpdate()` forces a fresh calculation each call, and this port
/// reproduces that with [`Instrument::recalculate`] before reading the legs.
///
/// The indexed-vs-at-par coupon mode is not read from a global singleton: the
/// helper carries the C++ `useIndexedCoupons_` `optional<bool>` (default `None`)
/// and forwards it to [`MakeVanillaSwap::with_indexed_coupons`], which resolves
/// `None` against [`Settings::using_at_par_coupons`] (D5, #315/#342).
pub struct SwapRateHelper {
    base: BootstrapHelperBase,
    swap: RefCell<Option<VanillaSwap>>,
    ibor_index: Shared<IborIndex>,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
    discount_relinkable_handle: RelinkableHandle<dyn YieldTermStructure>,
    discount_handle: Option<Handle<dyn YieldTermStructure>>,
    spread: Handle<dyn Quote>,
    settings: Shared<Settings<Date>>,
    tenor: Period,
    forward_start: Period,
    calendar: Calendar,
    fixed_frequency: Frequency,
    fixed_convention: BusinessDayConvention,
    fixed_day_count: DayCounter,
    end_of_month: bool,
    use_indexed_coupons: Option<bool>,
    pillar: Pillar,
}

impl SwapRateHelper {
    /// A swap helper fitting `quote` with the schedule of a spot-starting swap
    /// of `tenor`, the form the curve-consistency oracle builds
    /// (`piecewiseyieldcurve.cpp:293`): no spread, no forward start, no exogenous
    /// discounting curve, and the default [`Pillar::LastRelevantDate`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        quote: Handle<dyn Quote>,
        tenor: Period,
        calendar: Calendar,
        fixed_frequency: Frequency,
        fixed_convention: BusinessDayConvention,
        fixed_day_count: DayCounter,
        ibor_index: &IborIndex,
    ) -> Shared<SwapRateHelper> {
        Self::build(
            quote,
            tenor,
            calendar,
            fixed_frequency,
            fixed_convention,
            fixed_day_count,
            ibor_index,
            Handle::empty(),
            Period::new(0, TimeUnit::Days),
            None,
            Pillar::LastRelevantDate,
        )
    }

    /// A swap helper fitting a fixed `rate`, wrapped in a [`SimpleQuote`].
    #[allow(clippy::too_many_arguments)]
    pub fn from_rate(
        rate: Real,
        tenor: Period,
        calendar: Calendar,
        fixed_frequency: Frequency,
        fixed_convention: BusinessDayConvention,
        fixed_day_count: DayCounter,
        ibor_index: &IborIndex,
    ) -> Shared<SwapRateHelper> {
        let quote = Handle::new(shared(SimpleQuote::new(rate)) as Shared<dyn Quote>);
        Self::new(
            quote,
            tenor,
            calendar,
            fixed_frequency,
            fixed_convention,
            fixed_day_count,
            ibor_index,
        )
    }

    /// The full constructor of the ported (tenor-based) form: a market `spread`
    /// handle (empty for none), a `forward_start`, an optional exogenous
    /// `discounting_curve`, and the [`Pillar`] choice.
    #[allow(clippy::too_many_arguments)]
    pub fn with_details(
        quote: Handle<dyn Quote>,
        tenor: Period,
        calendar: Calendar,
        fixed_frequency: Frequency,
        fixed_convention: BusinessDayConvention,
        fixed_day_count: DayCounter,
        ibor_index: &IborIndex,
        spread: Handle<dyn Quote>,
        forward_start: Period,
        discounting_curve: Option<Handle<dyn YieldTermStructure>>,
        pillar: Pillar,
    ) -> Shared<SwapRateHelper> {
        Self::build(
            quote,
            tenor,
            calendar,
            fixed_frequency,
            fixed_convention,
            fixed_day_count,
            ibor_index,
            spread,
            forward_start,
            discounting_curve,
            pillar,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        quote: Handle<dyn Quote>,
        tenor: Period,
        calendar: Calendar,
        fixed_frequency: Frequency,
        fixed_convention: BusinessDayConvention,
        fixed_day_count: DayCounter,
        source_index: &IborIndex,
        spread: Handle<dyn Quote>,
        forward_start: Period,
        discounting_curve: Option<Handle<dyn YieldTermStructure>>,
        pillar: Pillar,
    ) -> Shared<SwapRateHelper> {
        let settings = source_index.base().settings().clone();
        Shared::new_cyclic(|weak: &Weak<SwapRateHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                }
            });
            let term_structure_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
            let ibor_index = shared(source_index.clone_with(term_structure_handle.handle()));
            let base = BootstrapHelperBase::new_relative(
                quote,
                Shared::clone(&settings),
                true,
                on_eval_change,
            );
            let helper = SwapRateHelper {
                base,
                swap: RefCell::new(None),
                ibor_index,
                term_structure_handle,
                discount_relinkable_handle: RelinkableHandle::<dyn YieldTermStructure>::empty(),
                discount_handle: discounting_curve,
                spread,
                settings,
                tenor,
                forward_start,
                calendar,
                fixed_frequency,
                fixed_convention,
                fixed_day_count,
                end_of_month: false,
                use_indexed_coupons: None,
                pillar,
            };
            helper.initialize_dates();
            helper
        })
    }
}

impl AsObservable for SwapRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for SwapRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    /// The par swap rate implied by the current curve
    /// (`ratehelpers.cpp:633-646`).
    ///
    /// The swap is force-recalculated first (the C++ `swap_->deepUpdate()`,
    /// forced because the helper does not observe the curve); then the rate is
    /// reconstructed from the floating-leg NPV, the spread carried on the
    /// floating-leg BPS, and the fixed-leg BPS, rather than read from
    /// `fair_rate()`.
    fn implied_quote(&self) -> QlResult<Real> {
        self.base.term_structure()?;
        let mut guard = self.swap.borrow_mut();
        let swap = guard
            .as_mut()
            .expect("initialize_dates populates the swap at construction");
        swap.recalculate()?;

        const BASIS_POINT: Real = 1.0e-4;
        let floating_leg_npv = swap.fixed_vs_floating_mut().floating_leg_npv()?;
        let spread = if self.spread.is_empty() {
            0.0
        } else {
            self.spread.current_link()?.value()?
        };
        let spread_npv = swap.fixed_vs_floating_mut().floating_leg_bps()? / BASIS_POINT * spread;
        let total_npv = -(floating_leg_npv + spread_npv);
        let fixed_leg_bps = swap.fixed_vs_floating_mut().fixed_leg_bps()?;
        Ok(total_npv / (fixed_leg_bps / BASIS_POINT))
    }

    /// Weak-links both the forecasting handle (used by the cloned index) and the
    /// discounting handle to the bootstrapping curve - or, when an exogenous
    /// discounting curve was supplied, links the discounting handle to that
    /// instead - then records the curve on the base (`ratehelpers.cpp:614`). All
    /// links are non-owning and unobserved.
    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        match &self.discount_handle {
            Some(discount) if !discount.is_empty() => {
                let curve = discount
                    .current_link()
                    .expect("a non-empty discount handle resolves");
                self.discount_relinkable_handle
                    .link_to_weak(Shared::downgrade(&curve));
            }
            _ => self
                .discount_relinkable_handle
                .link_to_weak(Shared::downgrade(term_structure)),
        }
        self.base.set_term_structure(term_structure);
    }
}

impl RelativeDateRateHelper for SwapRateHelper {
    /// Rebuilds the swap and its schedule off the current evaluation date
    /// (`initializeDates`, `ratehelpers.cpp:530`): a spot-starting swap of the
    /// helper's tenor, built through [`MakeVanillaSwap`] with a 0% fixed rate so
    /// it does not price at construction. Earliest and maturity come from the
    /// leg schedules; the pillar follows the [`Pillar`] choice.
    ///
    /// The `latest_relevant_date` is set to the maturity. C++ takes the maximum
    /// of the maturity and the last floating coupon's `fixingEndDate`; that
    /// refinement needs an `IborCoupon` fixing-end-date accessor the cash-flow
    /// surface does not yet expose, so it is deferred to the bootstrap ticket
    /// (#341) that exercises pillar ordering.
    fn initialize_dates(&self) {
        let fixed_tenor = if self.fixed_frequency == Frequency::Once {
            self.tenor
        } else {
            Period::try_from(self.fixed_frequency)
                .expect("a swap's fixed frequency maps to a valid period")
        };
        let swap = MakeVanillaSwap::new(
            self.tenor,
            Shared::clone(&self.ibor_index),
            Some(0.0),
            self.forward_start,
            Shared::clone(&self.settings),
        )
        .with_discounting_term_structure(self.discount_relinkable_handle.handle())
        .with_fixed_leg_day_count(self.fixed_day_count.clone())
        .with_fixed_leg_tenor(fixed_tenor)
        .with_fixed_leg_convention(self.fixed_convention)
        .with_fixed_leg_termination_date_convention(self.fixed_convention)
        .with_fixed_leg_calendar(self.calendar.clone())
        .with_fixed_leg_end_of_month(self.end_of_month)
        .with_floating_leg_calendar(self.calendar.clone())
        .with_floating_leg_end_of_month(self.end_of_month)
        .with_indexed_coupons(self.use_indexed_coupons)
        .build()
        .expect("a 0% fixed-rate swap with a valid evaluation date builds without pricing");

        let base = swap.fixed_vs_floating();
        let earliest = base
            .fixed_schedule()
            .start_date()
            .min(base.floating_schedule().start_date());
        let maturity = base
            .fixed_schedule()
            .end_date()
            .max(base.floating_schedule().end_date());

        let latest_relevant = maturity;
        self.base.set_earliest_date(earliest);
        self.base.set_maturity_date(maturity);
        self.base.set_latest_relevant_date(latest_relevant);
        let pillar = match self.pillar {
            Pillar::MaturityDate => maturity,
            Pillar::LastRelevantDate => latest_relevant,
        };
        self.base.set_pillar_date(pillar);
        self.base.set_latest_date(pillar);

        *self.swap.borrow_mut() = Some(swap);
    }
}

/// Bootstrap helper over an overnight-indexed-swap (OIS) rate (`OISRateHelper`).
///
/// The helper fits the fixed rate at which an OIS of the quoted tenor is worth
/// par on the bootstrapping curve. Its own swap is built through [`MakeOis`]
/// (`oisratehelper.cpp:130`) off a cloned overnight index and a relinkable
/// discounting handle, so it prices against the curve it is being bootstrapped
/// against.
///
/// [`implied_quote`](RateHelper::implied_quote) is the par-rate reconstruction
/// of `oisratehelper.cpp:220-232` (the same shape as [`SwapRateHelper`]'s, not
/// `fair_rate()`): over the overnight-leg NPV and BPS and the fixed-leg BPS,
/// carrying the spread term. Because the helper deliberately does not observe
/// the curve (its pricing handles are weak-linked, unobserved), the swap's
/// cached results go stale when the bootstrap moves the curve; the C++
/// `swap_->deepUpdate()` forces a fresh calculation each call, reproduced here
/// with [`Instrument::recalculate`] before reading the legs.
pub struct OISRateHelper {
    base: BootstrapHelperBase,
    swap: RefCell<Option<OvernightIndexedSwap>>,
    overnight_index: Shared<OvernightIndex>,
    term_structure_handle: RelinkableHandle<dyn YieldTermStructure>,
    discount_relinkable_handle: RelinkableHandle<dyn YieldTermStructure>,
    discount_handle: Option<Handle<dyn YieldTermStructure>>,
    overnight_spread: Handle<dyn Quote>,
    settings: Shared<Settings<Date>>,
    settlement_days: Natural,
    tenor: Period,
    forward_start: Period,
    payment_lag: Integer,
    payment_convention: BusinessDayConvention,
    payment_frequency: Frequency,
    averaging_method: RateAveraging,
    pillar: Pillar,
}

impl OISRateHelper {
    /// An OIS helper fitting `quote` with the schedule of a swap of `tenor`
    /// starting `settlement_days` after the evaluation date, the form the
    /// bootstrap oracle builds (`overnightindexedswap.cpp:236-256`).
    ///
    /// `discounting_curve` is the exogenous discounting curve (empty `None` to
    /// discount off the bootstrapping curve). `overnight_spread` is the market
    /// spread handle (empty for none). The deferred knobs the C++ constructor
    /// carries past `averaging_method` (telescopic value dates, lookback,
    /// lockout, observation shift, custom pillar, per-leg calendars) take their
    /// benign defaults, mirroring the oracle's positional construction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        tenor: Period,
        quote: Handle<dyn Quote>,
        overnight_index: &OvernightIndex,
        discounting_curve: Option<Handle<dyn YieldTermStructure>>,
        payment_lag: Integer,
        payment_convention: BusinessDayConvention,
        payment_frequency: Frequency,
        forward_start: Period,
        overnight_spread: Handle<dyn Quote>,
        pillar: Pillar,
        averaging_method: RateAveraging,
        settings: Shared<Settings<Date>>,
    ) -> Shared<OISRateHelper> {
        Shared::new_cyclic(|weak: &Weak<OISRateHelper>| {
            let weak = weak.clone();
            let on_eval_change = Box::new(move || {
                if let Some(helper) = weak.upgrade() {
                    helper.initialize_dates();
                }
            });
            let term_structure_handle = RelinkableHandle::<dyn YieldTermStructure>::empty();
            let cloned_index = overnight_index.clone_with(term_structure_handle.handle());
            let base = BootstrapHelperBase::new_relative(
                quote,
                Shared::clone(&settings),
                true,
                on_eval_change,
            );
            let helper = OISRateHelper {
                base,
                swap: RefCell::new(None),
                overnight_index: cloned_index,
                term_structure_handle,
                discount_relinkable_handle: RelinkableHandle::<dyn YieldTermStructure>::empty(),
                discount_handle: discounting_curve,
                overnight_spread,
                settings,
                settlement_days,
                tenor,
                forward_start,
                payment_lag,
                payment_convention,
                payment_frequency,
                averaging_method,
                pillar,
            };
            helper.initialize_dates();
            helper
        })
    }
}

impl AsObservable for OISRateHelper {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl RateHelper for OISRateHelper {
    fn base(&self) -> &BootstrapHelperBase {
        &self.base
    }

    /// The par OIS rate implied by the current curve (`oisratehelper.cpp:220-232`).
    ///
    /// The swap is force-recalculated first (the C++ `swap_->deepUpdate()`,
    /// forced because the helper does not observe the curve); then the rate is
    /// reconstructed from the overnight-leg NPV, the spread carried on the
    /// overnight-leg BPS, and the fixed-leg BPS, rather than read from
    /// `fair_rate()`.
    fn implied_quote(&self) -> QlResult<Real> {
        self.base.term_structure()?;
        let mut guard = self.swap.borrow_mut();
        let swap = guard
            .as_mut()
            .expect("initialize_dates populates the swap at construction");
        swap.recalculate()?;

        const BASIS_POINT: Real = 1.0e-4;
        let overnight_leg_npv = swap.overnight_leg_npv()?;
        let spread = if self.overnight_spread.is_empty() {
            0.0
        } else {
            self.overnight_spread.current_link()?.value()?
        };
        let spread_npv = swap.overnight_leg_bps()? / BASIS_POINT * spread;
        let total_npv = -(overnight_leg_npv + spread_npv);
        let fixed_leg_bps = swap.fixed_vs_floating_mut().fixed_leg_bps()?;
        Ok(total_npv / (fixed_leg_bps / BASIS_POINT))
    }

    /// Weak-links the forecasting handle (used by the cloned overnight index) and
    /// the discounting handle to the bootstrapping curve - or, when an exogenous
    /// discounting curve was supplied, links the discounting handle to that
    /// instead - then records the curve on the base (`oisratehelper.cpp:198-210`).
    /// All links are non-owning and unobserved.
    fn set_term_structure(&self, term_structure: &Shared<dyn YieldTermStructure>) {
        self.term_structure_handle
            .link_to_weak(Shared::downgrade(term_structure));
        match &self.discount_handle {
            Some(discount) if !discount.is_empty() => {
                let curve = discount
                    .current_link()
                    .expect("a non-empty discount handle resolves");
                self.discount_relinkable_handle
                    .link_to_weak(Shared::downgrade(&curve));
            }
            _ => self
                .discount_relinkable_handle
                .link_to_weak(Shared::downgrade(term_structure)),
        }
        self.base.set_term_structure(term_structure);
    }
}

impl RelativeDateRateHelper for OISRateHelper {
    /// Rebuilds the OIS and its schedule off the current evaluation date
    /// (`initializeDates`, `oisratehelper.cpp:128-193`): a swap of the helper's
    /// tenor built through [`MakeOis`]' whole builder chain with a 0% fixed rate
    /// so it does not price at construction.
    ///
    /// The `latest_relevant_date` is `max(maturity, lastPaymentDate)`.  C++ also
    /// maxes in `fixingEndDate = overnightIndex.maturityDate(valueDate(
    /// lastFixingDate))` (`oisratehelper.cpp:170-172`); that term is dominated by
    /// `lastPaymentDate` whenever the payment lag is at least one business day
    /// (the bootstrap oracle uses lag 2), and reaching the last coupon's fixing
    /// date needs a typed accessor the `dyn CashFlow` leg does not expose, so it
    /// is deferred with the arithmetic-averaging leg.
    fn initialize_dates(&self) {
        let swap = MakeOis::new(
            self.tenor,
            Shared::clone(&self.overnight_index),
            Some(0.0),
            self.forward_start,
            Shared::clone(&self.settings),
        )
        .with_discounting_term_structure(self.discount_relinkable_handle.handle())
        .with_telescopic_value_dates(false)
        .with_payment_lag(self.payment_lag)
        .with_payment_adjustment(self.payment_convention)
        .with_payment_frequency(self.payment_frequency)
        .with_averaging_method(self.averaging_method)
        .with_lookback_days(None)
        .with_lockout_days(0)
        .with_rule(DateGeneration::Backward)
        .with_convention(BusinessDayConvention::ModifiedFollowing)
        .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
        .with_observation_shift(false)
        .with_settlement_days(self.settlement_days)
        .build()
        .expect("a 0% fixed-rate OIS with benign deferred knobs builds without pricing");

        let base_swap = swap.fixed_vs_floating();
        let earliest = swap
            .overnight_schedule()
            .start_date()
            .min(base_swap.fixed_schedule().start_date());
        let maturity = swap
            .overnight_schedule()
            .end_date()
            .max(base_swap.fixed_schedule().end_date());

        let last_overnight_payment = swap.overnight_leg().last().map_or(maturity, |cf| cf.date());
        let last_fixed_payment = base_swap
            .fixed_leg()
            .last()
            .map_or(maturity, |cf| cf.date());
        let latest_relevant = maturity.max(last_overnight_payment).max(last_fixed_payment);

        self.base.set_earliest_date(earliest);
        self.base.set_maturity_date(maturity);
        self.base.set_latest_relevant_date(latest_relevant);
        self.base.set_latest_date(latest_relevant);
        let pillar = match self.pillar {
            Pillar::MaturityDate => maturity,
            Pillar::LastRelevantDate => latest_relevant,
        };
        self.base.set_pillar_date(pillar);

        *self.swap.borrow_mut() = Some(swap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::{currency::Currency, types::Rate};

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    fn euribor(
        tenor: Period,
        forwarding: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> IborIndex {
        IborIndex::new(
            "Euribor".into(),
            tenor,
            2,
            Currency::eur(),
            Target::new(),
            BusinessDayConvention::Following,
            false,
            Actual360::new(),
            forwarding,
            settings,
        )
    }

    fn flat_curve(reference: Date, rate: Rate) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    /// Standalone oracle: off a flat continuously-compounded curve the implied
    /// deposit rate is the closed-form simple forward `(exp(r*t) - 1)/t` over
    /// the helper's value-to-maturity window, and equals the index fixing path.
    #[test]
    fn implied_quote_matches_closed_form_deposit_rate() {
        let settings = settings_on(today());
        let source = euribor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let helper = DepositRateHelper::from_rate(0.02, &source);

        let rate = 0.03;
        let curve = flat_curve(today(), rate);
        helper.set_term_structure(&curve);

        let d1 = helper.earliest_date();
        let d2 = helper.maturity_date();
        let t = Actual360::new().year_fraction(d1, d2);
        let implied = helper.implied_quote().unwrap();

        let closed_form = ((rate * t).exp() - 1.0) / t;
        assert!((implied - closed_form).abs() < 1e-12);
    }

    /// `initializeDates` derives earliest/maturity/pillar from the index
    /// conventions off the evaluation date (`ratehelpers.cpp:228`).
    #[test]
    fn initialize_dates_follows_the_index_conventions() {
        let settings = settings_on(today());
        let source = euribor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let helper = DepositRateHelper::from_rate(0.02, &source);

        let reference = source
            .fixing_calendar()
            .adjust(today(), BusinessDayConvention::Following);
        let earliest = source.value_date(reference).unwrap();
        let maturity = source.maturity_date(earliest).unwrap();

        assert_eq!(helper.earliest_date(), earliest);
        assert!(earliest > today(), "the value date is spot, past today");
        assert_eq!(helper.maturity_date(), maturity);
        assert_eq!(helper.pillar_date(), maturity);
        assert_eq!(helper.latest_relevant_date(), maturity);
    }

    /// The clone mechanism: the helper forecasts off its OWN handle (the curve
    /// it is bootstrapped against), leaving the source index - here on an empty
    /// handle - untouched.
    #[test]
    fn helper_prices_off_its_own_handle_not_the_source_index() {
        let settings = settings_on(today());
        let source = euribor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let helper = DepositRateHelper::from_rate(0.02, &source);

        let curve = flat_curve(today(), 0.03);
        helper.set_term_structure(&curve);
        let implied_low = helper.implied_quote().unwrap();

        let curve_high = flat_curve(today(), 0.06);
        helper.set_term_structure(&curve_high);
        let implied_high = helper.implied_quote().unwrap();

        assert!(
            implied_high > implied_low,
            "relinking the helper's handle moves its implied quote"
        );
        assert!(
            source.forecast_fixing(helper.earliest_date()).is_err(),
            "the source index's own empty handle is untouched"
        );
    }

    /// `quote_error` is market minus implied.
    #[test]
    fn quote_error_is_market_minus_implied() {
        let settings = settings_on(today());
        let source = euribor(Period::new(6, TimeUnit::Months), Handle::empty(), settings);
        let helper = DepositRateHelper::from_rate(0.05, &source);

        let curve = flat_curve(today(), 0.03);
        helper.set_term_structure(&curve);

        let implied = helper.implied_quote().unwrap();
        assert!((helper.quote_error().unwrap() - (0.05 - implied)).abs() < 1e-15);
    }

    /// An evaluation-date change reruns `initializeDates` and notifies observers.
    #[test]
    fn evaluation_date_change_reinitializes_dates() {
        let settings = settings_on(today());
        let source = euribor(
            Period::new(6, TimeUnit::Months),
            Handle::empty(),
            settings.clone(),
        );
        let helper = DepositRateHelper::from_rate(0.02, &source);
        let before = helper.earliest_date();

        let flag = Flag::new();
        helper.observable().register_observer(&as_observer(&flag));

        let moved = today() + 30;
        settings.set_evaluation_date(moved);

        assert!(Flag::is_up(&flag), "date change must notify observers");
        assert!(
            helper.earliest_date() > before,
            "date change must rerun initialize_dates"
        );
    }

    fn swap_setup() -> (Shared<Settings<Date>>, IborIndex) {
        let settings = settings_on(today());
        let source = euribor(
            Period::new(6, TimeUnit::Months),
            Handle::empty(),
            settings.clone(),
        );
        (settings, source)
    }

    /// Builds the same spot-starting swap the helper builds, directly on `curve`,
    /// for a cross-implementation identity: a separate [`MakeVanillaSwap`]
    /// instance priced by its own [`DiscountingSwapEngine`].
    fn independent_swap(
        source: &IborIndex,
        tenor: Period,
        calendar: Calendar,
        convention: BusinessDayConvention,
        curve: &Shared<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> VanillaSwap {
        let curve_handle = Handle::new(Shared::clone(curve));
        let index = shared(source.clone_with(curve_handle.clone()));
        MakeVanillaSwap::new(
            tenor,
            index,
            Some(0.0),
            Period::new(0, TimeUnit::Days),
            settings,
        )
        .with_discounting_term_structure(curve_handle)
        .with_fixed_leg_day_count(Actual360::new())
        .with_fixed_leg_tenor(Period::try_from(Frequency::Annual).unwrap())
        .with_fixed_leg_convention(convention)
        .with_fixed_leg_termination_date_convention(convention)
        .with_fixed_leg_calendar(calendar.clone())
        .with_fixed_leg_end_of_month(false)
        .with_floating_leg_calendar(calendar)
        .with_floating_leg_end_of_month(false)
        .build()
        .unwrap()
    }

    /// With no spread, the reconstructed `implied_quote` equals the fair rate of
    /// the same swap computed independently - the par-rate formula reduces to the
    /// fair rate exactly.
    #[test]
    fn implied_quote_matches_fair_rate_of_the_same_swap() {
        let (settings, source) = swap_setup();
        let tenor = Period::new(5, TimeUnit::Years);
        let calendar = Target::new();
        let convention = BusinessDayConvention::ModifiedFollowing;
        let helper = SwapRateHelper::from_rate(
            0.02,
            tenor,
            calendar.clone(),
            Frequency::Annual,
            convention,
            Actual360::new(),
            &source,
        );
        let curve = flat_curve(today(), 0.03);
        helper.set_term_structure(&curve);

        let implied = helper.implied_quote().unwrap();
        let mut independent =
            independent_swap(&source, tenor, calendar, convention, &curve, settings);
        let fair = independent.fixed_vs_floating_mut().fair_rate().unwrap();
        assert!(
            (implied - fair).abs() < 1e-12,
            "implied {implied} vs fair {fair}"
        );
    }

    /// A nonzero spread shifts the implied quote off the fair rate by exactly
    /// `spread * floatingLegBPS / fixedLegBPS` (BPS from an independent swap) -
    /// the term a bare `fair_rate()` would drop.
    #[test]
    fn nonzero_spread_shifts_the_implied_quote_by_the_bps_ratio() {
        let (settings, source) = swap_setup();
        let tenor = Period::new(5, TimeUnit::Years);
        let calendar = Target::new();
        let convention = BusinessDayConvention::ModifiedFollowing;
        let curve = flat_curve(today(), 0.03);

        let helper0 = SwapRateHelper::from_rate(
            0.02,
            tenor,
            calendar.clone(),
            Frequency::Annual,
            convention,
            Actual360::new(),
            &source,
        );
        helper0.set_term_structure(&curve);
        let implied0 = helper0.implied_quote().unwrap();

        let spread = 0.001;
        let spread_handle = Handle::new(shared(SimpleQuote::new(spread)) as Shared<dyn Quote>);
        let helper_s = SwapRateHelper::with_details(
            Handle::new(shared(SimpleQuote::new(0.02)) as Shared<dyn Quote>),
            tenor,
            calendar.clone(),
            Frequency::Annual,
            convention,
            Actual360::new(),
            &source,
            spread_handle,
            Period::new(0, TimeUnit::Days),
            None,
            Pillar::LastRelevantDate,
        );
        helper_s.set_term_structure(&curve);
        let implied_s = helper_s.implied_quote().unwrap();

        assert!(
            (implied_s - implied0).abs() > 1e-8,
            "the spread must move the implied quote"
        );

        let mut independent =
            independent_swap(&source, tenor, calendar, convention, &curve, settings);
        let floating_bps = independent
            .fixed_vs_floating_mut()
            .floating_leg_bps()
            .unwrap();
        let fixed_bps = independent.fixed_vs_floating_mut().fixed_leg_bps().unwrap();
        let expected = implied0 - spread * floating_bps / fixed_bps;
        assert!(
            (implied_s - expected).abs() < 1e-12,
            "implied_s {implied_s} vs expected {expected}"
        );
    }

    /// The forced recalculation: moving the curve (mutating its underlying quote,
    /// not relinking) changes the implied quote even though the helper never
    /// observes the curve and receives no notification - proving `implied_quote`
    /// forces a fresh calculation rather than reading a stale cache.
    #[test]
    fn moving_the_curve_updates_the_quote_without_notifying_the_helper() {
        let (_settings, source) = swap_setup();
        let tenor = Period::new(5, TimeUnit::Years);
        let helper = SwapRateHelper::from_rate(
            0.02,
            tenor,
            Target::new(),
            Frequency::Annual,
            BusinessDayConvention::ModifiedFollowing,
            Actual360::new(),
            &source,
        );

        let quote = shared(SimpleQuote::new(0.03));
        let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::new(
            today(),
            Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        helper.set_term_structure(&curve);
        let implied_before = helper.implied_quote().unwrap();

        let flag = Flag::new();
        helper.observable().register_observer(&as_observer(&flag));

        quote.set_value(0.05);
        assert!(
            !Flag::is_up(&flag),
            "the helper must not observe the bootstrapping curve"
        );

        let implied_after = helper.implied_quote().unwrap();
        assert!(
            (implied_after - implied_before).abs() > 1e-6,
            "the forced recalculation must surface the curve move without a notification"
        );
    }

    /// `initialize_dates` builds a spot-starting swap and the pillar follows the
    /// [`Pillar`] choice.
    #[test]
    fn initialize_dates_spot_starts_and_pillar_follows_the_choice() {
        let (_settings, source) = swap_setup();
        let tenor = Period::new(5, TimeUnit::Years);
        let helper = SwapRateHelper::with_details(
            Handle::new(shared(SimpleQuote::new(0.02)) as Shared<dyn Quote>),
            tenor,
            Target::new(),
            Frequency::Annual,
            BusinessDayConvention::ModifiedFollowing,
            Actual360::new(),
            &source,
            Handle::empty(),
            Period::new(0, TimeUnit::Days),
            None,
            Pillar::MaturityDate,
        );
        assert!(
            helper.earliest_date() > today(),
            "the swap starts spot, past today"
        );
        assert!(helper.maturity_date() > helper.earliest_date());
        assert_eq!(
            helper.pillar_date(),
            helper.maturity_date(),
            "the MaturityDate pillar equals the maturity"
        );
    }

    /// `quote_error` is market minus implied.
    #[test]
    fn swap_quote_error_is_market_minus_implied() {
        let (_settings, source) = swap_setup();
        let tenor = Period::new(5, TimeUnit::Years);
        let helper = SwapRateHelper::from_rate(
            0.05,
            tenor,
            Target::new(),
            Frequency::Annual,
            BusinessDayConvention::ModifiedFollowing,
            Actual360::new(),
            &source,
        );
        let curve = flat_curve(today(), 0.03);
        helper.set_term_structure(&curve);

        let implied = helper.implied_quote().unwrap();
        assert!((helper.quote_error().unwrap() - (0.05 - implied)).abs() < 1e-15);
    }

    /// `overnightindexedswap.cpp estrSwapData` (`:92-125`): the OIS quotes the
    /// bootstrap oracle fits, `(tenor length, unit, rate %)`; all use two
    /// settlement days.
    const ESTR_SWAP_DATA: [(i32, TimeUnit, Real); 33] = [
        (1, TimeUnit::Weeks, 1.245),
        (2, TimeUnit::Weeks, 1.269),
        (3, TimeUnit::Weeks, 1.277),
        (1, TimeUnit::Months, 1.281),
        (2, TimeUnit::Months, 1.18),
        (3, TimeUnit::Months, 1.143),
        (4, TimeUnit::Months, 1.125),
        (5, TimeUnit::Months, 1.116),
        (6, TimeUnit::Months, 1.111),
        (7, TimeUnit::Months, 1.109),
        (8, TimeUnit::Months, 1.111),
        (9, TimeUnit::Months, 1.117),
        (10, TimeUnit::Months, 1.129),
        (11, TimeUnit::Months, 1.141),
        (12, TimeUnit::Months, 1.153),
        (15, TimeUnit::Months, 1.218),
        (18, TimeUnit::Months, 1.308),
        (21, TimeUnit::Months, 1.407),
        (2, TimeUnit::Years, 1.510),
        (3, TimeUnit::Years, 1.916),
        (4, TimeUnit::Years, 2.254),
        (5, TimeUnit::Years, 2.523),
        (6, TimeUnit::Years, 2.746),
        (7, TimeUnit::Years, 2.934),
        (8, TimeUnit::Years, 3.092),
        (9, TimeUnit::Years, 3.231),
        (10, TimeUnit::Years, 3.380),
        (11, TimeUnit::Years, 3.457),
        (12, TimeUnit::Years, 3.544),
        (15, TimeUnit::Years, 3.702),
        (20, TimeUnit::Years, 3.703),
        (25, TimeUnit::Years, 3.541),
        (30, TimeUnit::Years, 3.369),
    ];

    /// `overnightindexedswap.cpp testBaseBootstrap` (`:397` ->
    /// `testBootstrap(false, RateAveraging::Compound)`, `:208`): an Estr
    /// discounting curve bootstrapped purely from [`OISRateHelper`]s reprices
    /// every OIS quote to 1e-8.
    ///
    /// The deposit helpers of the C++ setup are omitted deliberately: for a zero
    /// spread `implied_quote == fair_rate`, and each swap tenor is self-pinned by
    /// its own OIS node solved so `implied_quote_i == quote_i`, so
    /// `fair_rate_i == quote_i` to solver accuracy independent of the sub-week
    /// short end the deposits shape. `paymentLag = 2` and `today = 5 Feb 2009`
    /// are transcribed from `CommonVars` / `testBootstrap` (`:180-215`).
    ///
    /// The three sibling cases stay deferred: `testBootstrapWithArithmeticAverage`
    /// (`:402`) needs the arithmetic-averaging pricer, and the two telescopic
    /// cases (`:407`/`:413`) need telescopic value dates - neither on main.
    #[test]
    fn ois_bootstrap_reprices_the_quotes() {
        use crate::indexes::ibor::Estr;
        use crate::math::interpolations::loglinear::LogLinear;
        use crate::termstructures::bootstraptraits::Discount;
        use crate::termstructures::yields::PiecewiseYieldCurve;
        use crate::time::daycounters::actual365fixed::Actual365Fixed;

        const PAYMENT_LAG: Integer = 2;
        let today = Date::new(5, Month::February, 2009);
        let settings = settings_on(today);
        let calendar = Target::new();
        let settlement = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        let estr = Estr::new(Handle::empty(), settings.clone());
        let mut instruments: Vec<Shared<dyn RateHelper>> = Vec::new();
        for (n, unit, rate) in ESTR_SWAP_DATA {
            let quote = Handle::new(shared(SimpleQuote::new(rate / 100.0)) as Shared<dyn Quote>);
            let helper = OISRateHelper::new(
                2,
                Period::new(n, unit),
                quote,
                &estr,
                None,
                PAYMENT_LAG,
                BusinessDayConvention::Following,
                Frequency::Annual,
                Period::new(0, TimeUnit::Days),
                Handle::empty(),
                Pillar::LastRelevantDate,
                RateAveraging::Compound,
                settings.clone(),
            );
            instruments.push(helper as Shared<dyn RateHelper>);
        }

        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            today,
            instruments,
            Actual365Fixed::new(),
            LogLinear,
        )
        .unwrap();
        let handle: Handle<dyn YieldTermStructure> =
            Handle::new(Shared::clone(&curve) as Shared<dyn YieldTermStructure>);

        for (n, unit, rate) in ESTR_SWAP_DATA {
            let priced_estr = shared(Estr::new(handle.clone(), settings.clone()));
            let mut swap = MakeOis::new(
                Period::new(n, unit),
                priced_estr,
                Some(0.0),
                Period::new(0, TimeUnit::Days),
                settings.clone(),
            )
            .with_effective_date(settlement)
            .with_nominal(100.0)
            .with_payment_lag(PAYMENT_LAG)
            .with_discounting_term_structure(handle.clone())
            .with_averaging_method(RateAveraging::Compound)
            .build()
            .unwrap();

            let calculated = swap.fixed_vs_floating_mut().fair_rate().unwrap();
            let expected = rate / 100.0;
            assert!(
                (calculated - expected).abs() < 1.0e-8,
                "{n} {unit:?} OIS: calculated {calculated} vs expected {expected}"
            );
        }
    }

    fn quote_handle(value: Real) -> Handle<dyn Quote> {
        Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
    }

    /// The convexity-shift pin (`ratehelpers.cpp:157`): two curves bootstrapped
    /// through a single [`FuturesRateHelper`] at the same price, one with zero
    /// convexity and one with `c`, produce forwards that differ by exactly `c`.
    ///
    /// Non-circular: the bootstrap only pins `implied_quote == price` at the
    /// pillar, which fixes `forward = (100 - price)/100 - conv_adj` per curve, so
    /// `forward_c == forward_0 - c` is a property of the convexity term itself,
    /// independent of the discount interpolation. Stubbing
    /// `convexity_adjustment()` to zero collapses both curves onto the same
    /// forward and this assertion fails.
    #[test]
    fn convexity_adjustment_shifts_the_bootstrapped_forward() {
        use crate::math::interpolations::loglinear::LogLinear;
        use crate::termstructures::bootstraptraits::Discount;
        use crate::termstructures::yields::PiecewiseYieldCurve;
        use crate::time::daycounters::actual365fixed::Actual365Fixed;

        let reference = today();
        let imm_start = imm::next_date(reference, false);
        let price = 96.0;
        let c = 0.001;

        let forward_from = |conv_adj: Handle<dyn Quote>| -> Real {
            let helper = FuturesRateHelper::from_end_date(
                quote_handle(price),
                imm_start,
                None,
                Actual360::new(),
                conv_adj,
                FuturesType::Imm,
            )
            .unwrap();
            let earliest = helper.earliest_date();
            let maturity = helper.maturity_date();
            let instruments: Vec<Shared<dyn RateHelper>> = vec![helper as Shared<dyn RateHelper>];
            let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
                reference,
                instruments,
                Actual365Fixed::new(),
                LogLinear,
            )
            .unwrap();
            let disc_e = curve.discount_date(earliest, false).unwrap();
            let disc_m = curve.discount_date(maturity, false).unwrap();
            let yf = Actual360::new().year_fraction_ref(earliest, maturity, earliest, maturity);
            (disc_e / disc_m - 1.0) / yf
        };

        let forward_0 = forward_from(Handle::empty());
        let forward_c = forward_from(quote_handle(c));
        assert!(
            (forward_c - (forward_0 - c)).abs() < 1e-10,
            "forward_c {forward_c} vs forward_0 - c {}",
            forward_0 - c
        );
    }

    /// The bootstrap reprices the futures quote (`ratehelpers.cpp:157`): the
    /// forward recomputed from the bootstrapped discounts reproduces the quoted
    /// price through `100 * (1 - forward - conv_adj)`, and [`implied_quote`] on
    /// the fitted curve returns the price to solver accuracy.
    ///
    /// [`implied_quote`]: RateHelper::implied_quote
    #[test]
    fn bootstrap_reprices_the_futures_quote() {
        use crate::math::interpolations::loglinear::LogLinear;
        use crate::termstructures::bootstraptraits::Discount;
        use crate::termstructures::yields::PiecewiseYieldCurve;
        use crate::time::daycounters::actual365fixed::Actual365Fixed;

        let reference = today();
        let imm_start = imm::next_date(reference, false);
        let price = 96.0;
        let c = 0.001;
        let helper = FuturesRateHelper::from_end_date(
            quote_handle(price),
            imm_start,
            None,
            Actual360::new(),
            quote_handle(c),
            FuturesType::Imm,
        )
        .unwrap();
        let earliest = helper.earliest_date();
        let maturity = helper.maturity_date();
        let instruments: Vec<Shared<dyn RateHelper>> =
            vec![Shared::clone(&helper) as Shared<dyn RateHelper>];
        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            reference,
            instruments,
            Actual365Fixed::new(),
            LogLinear,
        )
        .unwrap();

        let disc_e = curve.discount_date(earliest, false).unwrap();
        let disc_m = curve.discount_date(maturity, false).unwrap();
        let yf = Actual360::new().year_fraction_ref(earliest, maturity, earliest, maturity);
        let forward = (disc_e / disc_m - 1.0) / yf;
        let repriced = 100.0 * (1.0 - forward - c);
        assert!(
            (repriced - price).abs() < 1e-9,
            "repriced {repriced} vs {price}"
        );

        let implied = helper.implied_quote().unwrap();
        assert!(
            (implied - price).abs() < 1e-9,
            "implied {implied} vs {price}"
        );
    }

    /// The explicit-date constructor with no end date advances three IMM periods
    /// and pins every schedule date to that window (`ratehelpers.cpp:91`).
    /// Stubbing the three-period advance to a single one makes the expected
    /// maturity disagree and this assertion fails.
    #[test]
    fn from_end_date_advances_three_imm_periods_and_pins_the_schedule() {
        let imm_start = imm::next_date(today(), false);
        assert!(imm::is_imm_date(imm_start, false));
        let helper = FuturesRateHelper::from_end_date(
            quote_handle(96.0),
            imm_start,
            None,
            Actual360::new(),
            Handle::empty(),
            FuturesType::Imm,
        )
        .unwrap();

        let expected_maturity = imm::next_date(
            imm::next_date(imm::next_date(imm_start, false), false),
            false,
        );
        assert_eq!(helper.earliest_date(), imm_start);
        assert_eq!(helper.maturity_date(), expected_maturity);
        assert_eq!(helper.pillar_date(), expected_maturity);
        assert_eq!(helper.latest_date(), expected_maturity);
        assert_eq!(helper.latest_relevant_date(), expected_maturity);
    }

    /// A start that is not a valid IMM date is rejected under the IMM convention
    /// (`CheckDate`, `ratehelpers.cpp:46`).
    #[test]
    fn a_non_imm_start_is_rejected_under_the_imm_convention() {
        let start = today();
        assert!(
            !imm::is_imm_date(start, false),
            "the fixture start must not be an IMM date"
        );
        let result = FuturesRateHelper::from_end_date(
            quote_handle(96.0),
            start,
            None,
            Actual360::new(),
            Handle::empty(),
            FuturesType::Imm,
        );
        assert!(result.is_err());
    }

    /// A Custom helper needs an explicit end date; the C++ null-maturity path is
    /// an error here (divergence documented on [`FuturesRateHelper::from_end_date`]).
    #[test]
    fn a_custom_helper_requires_an_explicit_end_date() {
        let start = today();
        let result = FuturesRateHelper::from_end_date(
            quote_handle(96.0),
            start,
            None,
            Actual360::new(),
            Handle::empty(),
            FuturesType::Custom,
        );
        assert!(result.is_err());
    }

    /// The index constructor takes its window from the index conventions: the
    /// maturity is the start advanced by the index tenor on its fixing calendar
    /// (`ratehelpers.cpp:139`).
    #[test]
    fn from_index_takes_the_window_from_the_index_conventions() {
        let settings = settings_on(today());
        let index = euribor(Period::new(3, TimeUnit::Months), Handle::empty(), settings);
        let imm_start = imm::next_date(today(), false);
        let helper = FuturesRateHelper::from_index(
            quote_handle(96.0),
            imm_start,
            &index,
            Handle::empty(),
            FuturesType::Imm,
        )
        .unwrap();

        let expected_maturity = index.fixing_calendar().advance_by_period(
            imm_start,
            index.tenor(),
            index.business_day_convention(),
            false,
        );
        assert_eq!(helper.earliest_date(), imm_start);
        assert_eq!(helper.maturity_date(), expected_maturity);
    }
}
