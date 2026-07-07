//! Abstract instrument class.
//!
//! Port of `ql/instrument.{hpp,cpp}`. The C++ `Instrument` is a `LazyObject`
//! whose `performCalculations` delegates to a `PricingEngine`; here the
//! [`Instrument`] trait carries the virtuals (`is_expired`,
//! `setup_arguments`, `fetch_results`, ...) with the C++ base behaviour as
//! default methods, and [`InstrumentBase`] is the embedded state: the
//! lazy-calculation core, the engine and the results fetched from it.
//!
//! Two deviations from C++, both by design decision: the constructor's
//! `registerWith(Settings::instance().evaluationDate())` has no singleton to
//! reach (D5), so the owner wires the instrument to its `Settings`
//! evaluation-date observable through [`InstrumentBase::observer`]; and the
//! `Null<Real>`/null-`Date` result sentinels become `Option` (D4/D5 idiom
//! used throughout the crate).

use std::any::Any;
use std::collections::BTreeMap;

use crate::errors::QlResult;
use crate::fail;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{Observable, Observer};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::shared::{Shared, SharedMut, shared_mut};
use crate::time::date::Date;
use crate::types::Real;

/// Results every instrument fetches back from its engine (the C++
/// `Instrument::results`).
///
/// Engines whose instrument needs no further outputs use it as their result
/// bundle directly; richer bundles embed one and hand it to
/// [`InstrumentBase::store_results`] from their `fetch_results` override.
#[derive(Default)]
pub struct InstrumentResults {
    /// The net present value.
    pub value: Option<Real>,
    /// The error estimate on the NPV, when the engine provides one.
    pub error_estimate: Option<Real>,
    /// The date the net present value refers to.
    pub valuation_date: Option<Date>,
    /// Any additional results returned by the engine, keyed by tag.
    pub additional_results: BTreeMap<String, Shared<dyn Any>>,
}

impl Results for InstrumentResults {
    fn reset(&mut self) {
        self.value = None;
        self.error_estimate = None;
        self.valuation_date = None;
        self.additional_results.clear();
    }
}

/// Observer half of an instrument: feeds input notifications into the lazy
/// core (the C++ `LazyObject::update` reached through `registerWith`).
struct Updater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for Updater {
    fn update(&mut self) {
        self.lazy.borrow_mut().on_update();
    }
}

/// State embedded by every concrete instrument: the lazy-calculation core,
/// the pricing engine and the results of the last calculation.
pub struct InstrumentBase {
    lazy: SharedMut<LazyObject>,
    updater: SharedMut<Updater>,
    engine: Option<SharedMut<dyn PricingEngine>>,
    results: InstrumentResults,
}

impl Default for InstrumentBase {
    fn default() -> Self {
        InstrumentBase::new()
    }
}

impl InstrumentBase {
    /// Creates the base with no engine attached.
    ///
    /// The lazy core forwards all notifications, the C++
    /// `LazyObject::Defaults` behaviour instruments are built against.
    pub fn new() -> Self {
        let lazy = shared_mut(LazyObject::new(true));
        let updater = shared_mut(Updater {
            lazy: SharedMut::clone(&lazy),
        });
        InstrumentBase {
            lazy,
            updater,
            engine: None,
            results: InstrumentResults::default(),
        }
    }

    /// The attached pricing engine, if any.
    pub fn pricing_engine(&self) -> Option<&SharedMut<dyn PricingEngine>> {
        self.engine.as_ref()
    }

    /// Sets the pricing engine, re-pointing the instrument's observation from
    /// the old engine to the new one and invalidating cached results
    /// (`Instrument::setPricingEngine`).
    pub fn set_pricing_engine(&mut self, engine: SharedMut<dyn PricingEngine>) {
        let observer = self.observer();
        if let Some(old) = &self.engine {
            old.borrow().observable().unregister_observer(&observer);
        }
        engine.borrow().observable().register_observer(&observer);
        self.engine = Some(engine);
        self.lazy.borrow_mut().on_update();
    }

    /// The instrument's observer half, for registering with inputs whose
    /// changes must invalidate cached results.
    ///
    /// The counterpart of the C++ `registerWith` calls; in particular the
    /// constructor's registration with the `Settings` evaluation date is,
    /// per D5, wired by the owner:
    /// `settings.register_eval_date_observer(&instrument.base().observer())`.
    pub fn observer(&self) -> SharedMut<dyn Observer> {
        SharedMut::clone(&self.updater) as SharedMut<dyn Observer>
    }

    /// Registers the instrument as an observer of `source`, a convenience
    /// over [`observer`](InstrumentBase::observer) for plain observables.
    pub fn register_with(&self, source: &Observable) -> bool {
        source.register_observer(&self.observer())
    }

    /// Registers a downstream observer of the instrument's own notifications.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.lazy.borrow().register_observer(observer)
    }

    /// Unregisters a downstream observer.
    pub fn unregister_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.lazy
            .borrow()
            .observable()
            .unregister_observer(observer)
    }

    /// Whether the cached results are currently valid.
    pub fn is_calculated(&self) -> bool {
        self.lazy.borrow().is_calculated()
    }

    /// The results of the last calculation.
    pub fn results(&self) -> &InstrumentResults {
        &self.results
    }

    /// Copies an engine's instrument-level results into the base, the shared
    /// tail of every `fetch_results` (the C++ `Instrument::fetchResults`).
    pub fn store_results(&mut self, results: &InstrumentResults) {
        self.results.value = results.value;
        self.results.error_estimate = results.error_estimate;
        self.results.valuation_date = results.valuation_date;
        self.results.additional_results = results.additional_results.clone();
    }
}

/// Interface of concrete instruments.
///
/// Mirrors the C++ `Instrument`: implementors embed an [`InstrumentBase`],
/// expose it through [`base`](Instrument::base)/[`base_mut`](Instrument::base_mut),
/// and provide [`is_expired`](Instrument::is_expired) plus - when priced by an
/// engine - [`setup_arguments`](Instrument::setup_arguments). The default
/// methods reproduce the C++ base behaviour: lazy caching around the engine's
/// reset / setup + validate / calculate / fetch protocol.
pub trait Instrument {
    /// The embedded base state.
    fn base(&self) -> &InstrumentBase;

    /// Mutable access to the embedded base state.
    fn base_mut(&mut self) -> &mut InstrumentBase;

    /// Whether the instrument might have value greater than zero.
    fn is_expired(&self) -> bool;

    /// Fills the engine's argument bundle ahead of a calculation; mandatory
    /// when a pricing engine is used.
    fn setup_arguments(&self, _arguments: &mut dyn Arguments) -> QlResult<()> {
        fail!("Instrument::setup_arguments() not implemented");
    }

    /// Reads a calculation's outputs back from the engine's result bundle.
    ///
    /// The default expects the bundle to be an [`InstrumentResults`];
    /// instruments with richer bundles override this and feed the embedded
    /// instrument-level part to [`InstrumentBase::store_results`].
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        let Some(results) = (results as &dyn Any).downcast_ref::<InstrumentResults>() else {
            fail!("no results returned from pricing engine");
        };
        self.base_mut().store_results(results);
        Ok(())
    }

    /// Leaves the instrument in a consistent state when the expiration
    /// condition is met (`setupExpired`): zero value, cleared extras.
    fn setup_expired(&mut self) {
        let results = &mut self.base_mut().results;
        results.value = Some(0.0);
        results.error_estimate = Some(0.0);
        results.valuation_date = None;
        results.additional_results.clear();
    }

    /// Runs the engine protocol (`performCalculations`): reset, fill and
    /// validate the arguments, calculate, fetch the results. Override only
    /// when pricing without an engine.
    fn perform_calculations(&mut self) -> QlResult<()> {
        let Some(engine) = self.base().pricing_engine().cloned() else {
            fail!("null pricing engine");
        };
        let mut engine = engine.borrow_mut();
        engine.reset();
        let arguments = engine.arguments_mut();
        self.setup_arguments(arguments)?;
        arguments.validate()?;
        engine.calculate()?;
        self.fetch_results(engine.results())
    }

    /// Recomputes the results if the cache is stale, short-circuiting expired
    /// instruments (`Instrument::calculate`).
    fn calculate(&mut self) -> QlResult<()> {
        if self.base().is_calculated() {
            return Ok(());
        }
        let lazy = SharedMut::clone(&self.base().lazy);
        if self.is_expired() {
            self.setup_expired();
            lazy.borrow_mut().calculate(|| Ok(()))
        } else {
            lazy.borrow_mut().calculate(|| self.perform_calculations())
        }
    }

    /// The net present value of the instrument (`NPV()`).
    fn npv(&mut self) -> QlResult<Real> {
        self.calculate()?;
        let Some(value) = self.base().results.value else {
            fail!("NPV not provided");
        };
        Ok(value)
    }

    /// The error estimate on the NPV, when available (`errorEstimate()`).
    fn error_estimate(&mut self) -> QlResult<Real> {
        self.calculate()?;
        let Some(value) = self.base().results.error_estimate else {
            fail!("error estimate not provided");
        };
        Ok(value)
    }

    /// The date the net present value refers to (`valuationDate()`).
    fn valuation_date(&mut self) -> QlResult<Date> {
        self.calculate()?;
        let Some(date) = self.base().results.valuation_date else {
            fail!("valuation date not provided");
        };
        Ok(date)
    }

    /// An additional named result returned by the engine (`result<T>(tag)`).
    fn result<T: Any + Clone>(&mut self, tag: &str) -> QlResult<T>
    where
        Self: Sized,
    {
        self.calculate()?;
        let Some(value) = self.base().results.additional_results.get(tag) else {
            fail!("{tag} not provided");
        };
        let Some(value) = value.as_ref().downcast_ref::<T>() else {
            fail!("{tag} does not hold the requested type");
        };
        Ok(value.clone())
    }

    /// All additional results returned by the engine (`additionalResults()`).
    fn additional_results(&mut self) -> QlResult<&BTreeMap<String, Shared<dyn Any>>> {
        self.calculate()?;
        Ok(&self.base().results.additional_results)
    }
}
