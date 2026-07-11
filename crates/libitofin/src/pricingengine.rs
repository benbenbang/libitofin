//! Base for pricing engines.
//!
//! Port of `ql/pricingengine.hpp`. `PricingEngine` is the type-erased
//! protocol an [`Instrument`](crate::instrument::Instrument) drives:
//! fill the [`Arguments`], validate, calculate, read the [`Results`].
//! The C++ `dynamic_cast`s between the abstract bundles and the concrete
//! types an engine carries become [`Any`] downcasts through the traits'
//! `Any` supertrait.
//!
//! [`GenericEngine`] is the C++ template base of the same name: the typed
//! argument/result storage plus the observable plumbing every concrete
//! engine embeds.

use std::any::Any;

use crate::errors::QlResult;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::shared::{Shared, SharedMut};

/// Input bundle of a pricing engine (the C++ `PricingEngine::arguments`).
pub trait Arguments: Any {
    /// Checks that the filled-in arguments are complete and consistent.
    fn validate(&self) -> QlResult<()>;
}

/// Output bundle of a pricing engine (the C++ `PricingEngine::results`).
pub trait Results: Any {
    /// Clears the results ahead of a calculation.
    fn reset(&mut self);

    /// The instrument-level slice of the bundle, when the bundle carries one.
    ///
    /// The counterpart of the C++ `dynamic_cast<const Instrument::results*>`
    /// in the default `Instrument::fetchResults`, which succeeds for any
    /// bundle deriving from `Instrument::results`: richer bundles embedding an
    /// [`InstrumentResults`](crate::instrument::InstrumentResults) override
    /// this to expose it, and the default `fetch_results` then works for them
    /// exactly as in C++.
    fn as_instrument_results(&self) -> Option<&crate::instrument::InstrumentResults> {
        None
    }
}

/// Interface for pricing engines.
///
/// Mirrors the C++ `PricingEngine`, an `Observable`: an instrument
/// registers with the engine (through [`AsObservable`]) so that changes to
/// the engine or its inputs invalidate the instrument's cached results.
pub trait PricingEngine: AsObservable {
    /// Mutable access to the argument bundle the instrument fills in.
    fn arguments_mut(&mut self) -> &mut dyn Arguments;

    /// The results of the last calculation.
    fn results(&self) -> &dyn Results;

    /// Clears the results ahead of a calculation.
    fn reset(&mut self);

    /// Prices the filled-in arguments into the results.
    fn calculate(&mut self) -> QlResult<()>;
}

/// Typed storage and observable plumbing shared by concrete engines.
///
/// Mirrors the C++ `GenericEngine<ArgumentsType, ResultsType>`: carries the
/// argument and result bundles and the engine's observable, plus the observer
/// half (the C++ `update() { notifyObservers(); }`) as a forwarder to register
/// with the engine's inputs via [`register_with`](GenericEngine::register_with).
/// A concrete engine embeds one, delegates its [`PricingEngine`] accessors to
/// it, and only implements `calculate`.
pub struct GenericEngine<A, R> {
    arguments: A,
    results: R,
    observable: Shared<Observable>,
    forwarder: SharedMut<ResetThenNotify>,
}

impl<A: Arguments, R: Results> GenericEngine<A, R> {
    /// Creates the engine base around its argument and result bundles.
    pub fn new(arguments: A, results: R) -> Self {
        let (observable, forwarder) = ResetThenNotify::forwarder();
        GenericEngine {
            arguments,
            results,
            observable,
            forwarder,
        }
    }

    /// The typed argument bundle.
    pub fn arguments(&self) -> &A {
        &self.arguments
    }

    /// Mutable access to the typed argument bundle.
    pub fn arguments_mut(&mut self) -> &mut A {
        &mut self.arguments
    }

    /// The typed result bundle.
    pub fn results(&self) -> &R {
        &self.results
    }

    /// Mutable access to the typed result bundle.
    pub fn results_mut(&mut self) -> &mut R {
        &mut self.results
    }

    /// Clears the results ahead of a calculation.
    pub fn reset(&mut self) {
        self.results.reset();
    }

    /// Registers the engine as an observer of `source`, so the source's
    /// notifications are forwarded to the engine's own observers (the C++
    /// `registerWith` on an engine input).
    pub fn register_with(&self, source: &Observable) -> bool {
        source.register_observer(&(SharedMut::clone(&self.forwarder) as SharedMut<dyn Observer>))
    }

    /// The engine as an observer, for registering with an input that exposes
    /// only observer registration (the C++ `registerWith` on a `Handle`).
    pub fn observer(&self) -> SharedMut<dyn Observer> {
        SharedMut::clone(&self.forwarder) as SharedMut<dyn Observer>
    }
}

impl<A, R> AsObservable for GenericEngine<A, R> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::require;
    use crate::test_support::{Flag, as_observer};
    use crate::types::Real;

    #[derive(Default)]
    struct SumArguments {
        x: Option<Real>,
        y: Option<Real>,
    }

    impl Arguments for SumArguments {
        fn validate(&self) -> QlResult<()> {
            require!(self.x.is_some() && self.y.is_some(), "both terms required");
            Ok(())
        }
    }

    #[derive(Default)]
    struct SumResults {
        value: Option<Real>,
    }

    impl Results for SumResults {
        fn reset(&mut self) {
            self.value = None;
        }
    }

    struct SumEngine {
        base: GenericEngine<SumArguments, SumResults>,
    }

    impl SumEngine {
        fn new() -> Self {
            SumEngine {
                base: GenericEngine::new(SumArguments::default(), SumResults::default()),
            }
        }
    }

    impl AsObservable for SumEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl PricingEngine for SumEngine {
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
            self.base.arguments().validate()?;
            let x = self.base.arguments().x.expect("validated");
            let y = self.base.arguments().y.expect("validated");
            self.base.results_mut().value = Some(x + y);
            Ok(())
        }
    }

    /// Drives the protocol exactly as an instrument does: reach the concrete
    /// arguments through the type-erased trait object, validate, calculate,
    /// then read the concrete results back the same way.
    #[test]
    fn engine_protocol_round_trip_through_trait_objects() {
        let mut engine = SumEngine::new();
        let engine: &mut dyn PricingEngine = &mut engine;

        engine.reset();
        let arguments = engine.arguments_mut();
        let sum = (arguments as &mut dyn Any)
            .downcast_mut::<SumArguments>()
            .expect("engine carries SumArguments");
        sum.x = Some(2.0);
        sum.y = Some(3.0);
        arguments.validate().unwrap();
        engine.calculate().unwrap();

        let results = (engine.results() as &dyn Any)
            .downcast_ref::<SumResults>()
            .expect("engine carries SumResults");
        assert_eq!(results.value, Some(5.0));
    }

    #[test]
    fn validate_rejects_incomplete_arguments() {
        let mut engine = SumEngine::new();
        engine.base.arguments_mut().x = Some(1.0);

        let err = engine.base.arguments().validate().unwrap_err();
        assert_eq!(err.message(), "both terms required");
        assert!(engine.calculate().is_err());
    }

    #[test]
    fn reset_clears_previous_results() {
        let mut engine = SumEngine::new();
        engine.base.arguments_mut().x = Some(1.0);
        engine.base.arguments_mut().y = Some(1.0);
        engine.calculate().unwrap();
        assert_eq!(engine.base.results().value, Some(2.0));

        engine.reset();
        assert_eq!(engine.base.results().value, None);
    }

    /// The observer half of the C++ `GenericEngine`: a notification from a
    /// registered input reaches the engine's own observers.
    #[test]
    fn input_notifications_are_forwarded_to_engine_observers() {
        let engine = SumEngine::new();
        let input = Observable::new();
        assert!(engine.base.register_with(&input));

        let flag = Flag::new();
        engine.observable().register_observer(&as_observer(&flag));

        input.notify_observers();
        assert!(
            Flag::is_up(&flag),
            "input change must reach engine observers"
        );
    }
}
