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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use crate::patterns::observable::AsObservable;
    use crate::pricingengine::GenericEngine;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::require;
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;

    #[derive(Default)]
    struct MockArguments {
        market: Option<Real>,
    }

    impl Arguments for MockArguments {
        fn validate(&self) -> QlResult<()> {
            require!(self.market.is_some(), "market value not set");
            Ok(())
        }
    }

    /// Doubles the market value, so recomputations are visible in the NPV.
    struct MockEngine {
        base: GenericEngine<MockArguments, InstrumentResults>,
        calculations: Shared<Cell<usize>>,
        provide_npv: bool,
    }

    impl AsObservable for MockEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl PricingEngine for MockEngine {
        fn arguments_mut(&mut self) -> &mut dyn Arguments {
            self.base.arguments_mut()
        }

        fn results(&self) -> &dyn Results {
            self.base.results()
        }

        fn reset(&mut self) {
            self.base.reset();
        }

        fn calculate(&mut self) -> QlResult<()> {
            self.calculations.set(self.calculations.get() + 1);
            let market = self.base.arguments().market.expect("validated");
            let results = self.base.results_mut();
            if self.provide_npv {
                results.value = Some(2.0 * market);
                results.error_estimate = Some(0.0);
                results.valuation_date = Some(Date::new(7, Month::July, 2026));
                results
                    .additional_results
                    .insert("market".to_string(), shared(market) as Shared<dyn Any>);
            }
            Ok(())
        }
    }

    fn mock_engine(provide_npv: bool) -> (SharedMut<MockEngine>, Shared<Cell<usize>>) {
        let calculations = shared(Cell::new(0_usize));
        let engine = shared_mut(MockEngine {
            base: GenericEngine::new(MockArguments::default(), InstrumentResults::default()),
            calculations: Shared::clone(&calculations),
            provide_npv,
        });
        (engine, calculations)
    }

    struct MockInstrument {
        base: InstrumentBase,
        market: Shared<SimpleQuote>,
        expired: bool,
    }

    impl MockInstrument {
        /// An instrument observing its market quote directly, the usual
        /// `registerWith` wiring.
        fn new(market: Shared<SimpleQuote>) -> Self {
            let instrument = MockInstrument::unwired(market);
            instrument
                .base
                .register_with(instrument.market.observable());
            instrument
        }

        /// An instrument NOT observing the quote, to prove invalidation
        /// through other paths (the engine chain).
        fn unwired(market: Shared<SimpleQuote>) -> Self {
            MockInstrument {
                base: InstrumentBase::new(),
                market,
                expired: false,
            }
        }
    }

    impl Instrument for MockInstrument {
        fn base(&self) -> &InstrumentBase {
            &self.base
        }

        fn base_mut(&mut self) -> &mut InstrumentBase {
            &mut self.base
        }

        fn is_expired(&self) -> bool {
            self.expired
        }

        fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
            let Some(arguments) = (arguments as &mut dyn Any).downcast_mut::<MockArguments>()
            else {
                fail!("wrong argument type");
            };
            arguments.market = Some(self.market.value()?);
            Ok(())
        }
    }

    /// The ticket's oracle: cache-on-repeat, recompute-on-notify.
    #[test]
    fn lazy_npv_caches_and_recomputes_on_quote_change() {
        let market = shared(SimpleQuote::new(2.0));
        let mut instrument = MockInstrument::new(Shared::clone(&market));
        let (engine, calculations) = mock_engine(true);
        instrument.base_mut().set_pricing_engine(engine);

        assert_eq!(instrument.npv().unwrap(), 4.0);
        assert_eq!(calculations.get(), 1);

        assert_eq!(instrument.npv().unwrap(), 4.0);
        assert_eq!(calculations.get(), 1, "second NPV must hit the cache");

        market.set_value(3.0);
        assert!(!instrument.base().is_calculated());
        assert_eq!(instrument.npv().unwrap(), 6.0);
        assert_eq!(
            calculations.get(),
            2,
            "quote change must trigger a recalculation"
        );

        assert_eq!(instrument.error_estimate().unwrap(), 0.0);
        assert_eq!(
            instrument.valuation_date().unwrap(),
            Date::new(7, Month::July, 2026)
        );
    }

    /// The C++ instruments test: "observability of class instances is checked".
    #[test]
    fn instrument_notifies_downstream_observers_on_input_change() {
        let market = shared(SimpleQuote::new(2.0));
        let mut instrument = MockInstrument::new(Shared::clone(&market));
        let (engine, _) = mock_engine(true);
        instrument.base_mut().set_pricing_engine(engine);
        instrument.npv().unwrap();

        let flag = Flag::new();
        instrument.base().register_observer(&as_observer(&flag));

        market.set_value(3.0);
        assert!(
            Flag::is_up(&flag),
            "input change must reach instrument observers"
        );
    }

    #[test]
    fn set_pricing_engine_switches_observation_and_invalidates() {
        let market = shared(SimpleQuote::new(1.0));
        let mut instrument = MockInstrument::new(Shared::clone(&market));
        let (first, first_calls) = mock_engine(true);
        instrument.base_mut().set_pricing_engine(first.clone());
        instrument.npv().unwrap();

        let flag = Flag::new();
        instrument.base().register_observer(&as_observer(&flag));

        let (second, second_calls) = mock_engine(true);
        instrument.base_mut().set_pricing_engine(second.clone());
        assert!(
            Flag::is_up(&flag),
            "switching engines must notify observers"
        );
        assert!(!instrument.base().is_calculated());

        instrument.npv().unwrap();
        assert_eq!(first_calls.get(), 1);
        assert_eq!(second_calls.get(), 1, "the new engine must price");

        first.borrow().observable().notify_observers();
        assert!(
            instrument.base().is_calculated(),
            "old engine is unregistered"
        );

        second.borrow().observable().notify_observers();
        assert!(!instrument.base().is_calculated(), "new engine invalidates");
    }

    /// The C++ chain: quote -> engine (GenericEngine forwarder) -> instrument.
    #[test]
    fn quote_change_reaches_instrument_through_the_engine() {
        let market = shared(SimpleQuote::new(2.0));
        let mut instrument = MockInstrument::unwired(Shared::clone(&market));
        let (engine, calculations) = mock_engine(true);
        engine.borrow().base.register_with(market.observable());
        instrument.base_mut().set_pricing_engine(engine);

        assert_eq!(instrument.npv().unwrap(), 4.0);

        market.set_value(5.0);
        assert!(!instrument.base().is_calculated());
        assert_eq!(instrument.npv().unwrap(), 10.0);
        assert_eq!(calculations.get(), 2);
    }

    #[test]
    fn missing_engine_and_missing_npv_are_reported() {
        let market = shared(SimpleQuote::new(2.0));
        let mut instrument = MockInstrument::new(Shared::clone(&market));
        let err = instrument.npv().unwrap_err();
        assert_eq!(err.message(), "null pricing engine");

        let (engine, _) = mock_engine(false);
        instrument.base_mut().set_pricing_engine(engine);
        let err = instrument.npv().unwrap_err();
        assert_eq!(err.message(), "NPV not provided");
        let err = instrument.error_estimate().unwrap_err();
        assert_eq!(err.message(), "error estimate not provided");
    }

    #[test]
    fn expired_instrument_reports_zero_without_pricing() {
        let market = shared(SimpleQuote::new(2.0));
        let mut instrument = MockInstrument::new(Shared::clone(&market));
        let (engine, calculations) = mock_engine(true);
        instrument.base_mut().set_pricing_engine(engine);
        instrument.expired = true;

        assert_eq!(instrument.npv().unwrap(), 0.0);
        assert_eq!(instrument.error_estimate().unwrap(), 0.0);
        assert_eq!(calculations.get(), 0, "expired instruments never price");

        let err = instrument.valuation_date().unwrap_err();
        assert_eq!(err.message(), "valuation date not provided");
    }

    #[test]
    fn additional_results_round_trip_by_tag_and_type() {
        let market = shared(SimpleQuote::new(2.0));
        let mut instrument = MockInstrument::new(Shared::clone(&market));
        let (engine, _) = mock_engine(true);
        instrument.base_mut().set_pricing_engine(engine);

        assert_eq!(instrument.result::<Real>("market").unwrap(), 2.0);
        assert_eq!(instrument.additional_results().unwrap().len(), 1);

        let err = instrument.result::<Real>("absent").unwrap_err();
        assert_eq!(err.message(), "absent not provided");

        let err = instrument.result::<i32>("market").unwrap_err();
        assert_eq!(err.message(), "market does not hold the requested type");
    }

    #[test]
    fn failed_calculation_recovers_after_the_input_is_fixed() {
        let market = shared(SimpleQuote::default());
        let mut instrument = MockInstrument::new(Shared::clone(&market));
        let (engine, calculations) = mock_engine(true);
        instrument.base_mut().set_pricing_engine(engine);

        let err = instrument.npv().unwrap_err();
        assert_eq!(err.message(), "invalid SimpleQuote");
        assert!(!instrument.base().is_calculated());

        market.set_value(4.0);
        assert_eq!(instrument.npv().unwrap(), 8.0);
        assert_eq!(calculations.get(), 1);
    }

    /// The C++ constructor's `registerWith(Settings evaluation date)`, wired
    /// explicitly per D5.
    #[test]
    fn evaluation_date_change_invalidates_the_instrument() {
        let market = shared(SimpleQuote::new(2.0));
        let mut instrument = MockInstrument::new(Shared::clone(&market));
        let (engine, calculations) = mock_engine(true);
        instrument.base_mut().set_pricing_engine(engine);

        let mut settings: Settings<Date> = Settings::new();
        settings.set_evaluation_date(Date::new(7, Month::July, 2026));
        settings.register_eval_date_observer(&instrument.base().observer());

        instrument.npv().unwrap();
        settings.set_evaluation_date(Date::new(8, Month::July, 2026));
        assert!(!instrument.base().is_calculated());
        instrument.npv().unwrap();
        assert_eq!(calculations.get(), 2);
    }
}
