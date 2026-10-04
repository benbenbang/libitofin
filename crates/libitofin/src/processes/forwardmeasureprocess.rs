//! Validated horizon state and one-dimensional forward-measure process contract.
//!
//! Mirrors `ql/processes/forwardmeasureprocess.{hpp,cpp}`. Unlike QuantLib's
//! uninitialized default horizon, every horizon here is finite and nonnegative.

use std::cell::Cell;

use crate::errors::QlResult;
use crate::require;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::Time;

/// A one-dimensional process expressed under a `T`-forward measure.
pub trait ForwardMeasureProcess1D: StochasticProcess1D {
    /// Owned horizon state used by the process dynamics.
    fn forward_measure_state(&self) -> &ForwardMeasureTime;

    /// The forward-measure numeraire maturity `T`.
    fn forward_measure_time(&self) -> Time {
        self.forward_measure_state().get()
    }

    /// Updates the horizon and notifies process observers, including on an
    /// unchanged valid horizon, matching QuantLib's setter.
    ///
    /// # Errors
    ///
    /// Rejects negative or nonfinite times without changing state or notifying.
    fn set_forward_measure_time(&mut self, t: Time) -> QlResult<()> {
        self.forward_measure_state().set(t)?;
        self.observable().notify_observers();
        Ok(())
    }
}

/// Owned finite, nonnegative forward-measure horizon, measured in process years.
#[derive(Clone, Debug)]
pub struct ForwardMeasureTime {
    t: Cell<Time>,
}

impl ForwardMeasureTime {
    /// Constructs a horizon, including the valid time-zero horizon.
    ///
    /// # Errors
    ///
    /// Rejects negative or nonfinite times.
    pub fn new(t: Time) -> QlResult<Self> {
        validate_time(t)?;
        Ok(Self { t: Cell::new(t) })
    }

    /// Returns the current horizon.
    pub fn get(&self) -> Time {
        self.t.get()
    }

    /// Replaces the horizon without sending notifications. Concrete processes
    /// should expose the notifying trait setter instead of this state helper.
    ///
    /// # Errors
    ///
    /// Rejects negative or nonfinite times, preserving the previous horizon.
    pub fn set(&self, t: Time) -> QlResult<()> {
        validate_time(t)?;
        self.t.set(t);
        Ok(())
    }
}

fn validate_time(t: Time) -> QlResult<()> {
    require!(
        t.is_finite() && t >= 0.0,
        "forward-measure time must be finite and nonnegative"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::test_support::{Flag, as_observer};
    use crate::types::Real;

    struct ForwardProcess {
        horizon: ForwardMeasureTime,
        observable: Observable,
    }

    impl AsObservable for ForwardProcess {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl StochasticProcess1D for ForwardProcess {
        fn x0(&self) -> QlResult<Real> {
            Ok(0.0)
        }
        fn drift(&self, t: Time, _x: Real) -> QlResult<Real> {
            Ok(-(self.forward_measure_time() - t))
        }
        fn diffusion(&self, _t: Time, _x: Real) -> QlResult<Real> {
            Ok(1.0)
        }
    }

    impl ForwardMeasureProcess1D for ForwardProcess {
        fn forward_measure_state(&self) -> &ForwardMeasureTime {
            &self.horizon
        }
    }

    #[test]
    fn horizon_validation_preserves_previous_state() {
        let horizon = ForwardMeasureTime::new(5.0).unwrap();
        for invalid in [-1.0, Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(ForwardMeasureTime::new(invalid).is_err());
            assert!(horizon.set(invalid).is_err());
            assert_eq!(horizon.get(), 5.0);
        }
        horizon.set(0.0).unwrap();
        assert_eq!(horizon.get(), 0.0);
    }

    #[test]
    fn setter_notifies_and_changes_dynamics_atomically() {
        let mut process = ForwardProcess {
            horizon: ForwardMeasureTime::new(5.0).unwrap(),
            observable: Observable::new(),
        };
        let flag = Flag::new();
        process.observable().register_observer(&as_observer(&flag));
        let process_trait: &mut dyn ForwardMeasureProcess1D = &mut process;
        assert!(process_trait.set_forward_measure_time(Real::NAN).is_err());
        assert!(!Flag::is_up(&flag));
        assert_eq!(process_trait.drift(1.0, 0.0).unwrap(), -4.0);
        process_trait.set_forward_measure_time(2.0).unwrap();
        assert!(Flag::is_up(&flag));
        assert_eq!(process_trait.drift(1.0, 0.0).unwrap(), -1.0);
        Flag::lower(&flag);
        process_trait.set_forward_measure_time(2.0).unwrap();
        assert!(Flag::is_up(&flag));
    }
}
