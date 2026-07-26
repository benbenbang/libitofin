//! Cost function reparametrized over a free-parameter subset.
//!
//! Port of `ql/math/optimization/projectedcostfunction.{hpp,cpp}`. A
//! [`ProjectedCostFunction`] wraps a [`CostFunction`] together with a
//! [`Projection`] (a seed vector plus a fixed/free mask). The optimizer sees
//! only the free parameters: [`value`](ProjectedCostFunction::value) and
//! [`values`](ProjectedCostFunction::values) reinstate the fixed slots from the
//! seed (via [`Projection::include`]) before delegating to the wrapped
//! function. This is the class [`Projection`]'s module docs defer to.
//!
//! QuantLib's `Projection::project`/`include` are reachable with a
//! size-mismatched array and would trip the `assert!`s ported into
//! [`Projection`]. [`ProjectedCostFunction::new`] is therefore fallible: it
//! builds the [`Projection`] up front (validating the seed against the mask),
//! so the infallible [`CostFunction`] methods only ever `include` a free
//! vector of the right length and the asserts are unreachable from calibration
//! input (D4).

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::costfunction::CostFunction;
use crate::math::optimization::projection::Projection;
use crate::types::Real;

/// A cost function seen through a fixed/free parameter split.
pub struct ProjectedCostFunction<'a> {
    cost_function: &'a dyn CostFunction,
    projection: Projection,
}

impl<'a> ProjectedCostFunction<'a> {
    /// Wraps `cost_function`, holding fixed every entry of `parameter_values`
    /// whose `fix_parameters` flag is `true`.
    ///
    /// # Errors
    ///
    /// Fails when [`Projection::new`] rejects the seed/mask pair: mismatched
    /// lengths, or a fully fixed split with no free parameters.
    pub fn new(
        cost_function: &'a dyn CostFunction,
        parameter_values: &Array,
        fix_parameters: Vec<bool>,
    ) -> QlResult<Self> {
        Ok(ProjectedCostFunction {
            cost_function,
            projection: Projection::new(parameter_values, fix_parameters)?,
        })
    }

    /// The free subset drawn from the full parameter vector `parameters`.
    pub fn project(&self, parameters: &Array) -> Array {
        self.projection.project(parameters)
    }

    /// The full parameter vector rebuilt from the free `projected_parameters`,
    /// reinstating the fixed slots from the seed.
    pub fn include(&self, projected_parameters: &Array) -> Array {
        self.projection.include(projected_parameters)
    }
}

impl CostFunction for ProjectedCostFunction<'_> {
    fn values(&self, free_parameters: &Array) -> Array {
        self.cost_function
            .values(&self.projection.include(free_parameters))
    }

    fn value(&self, free_parameters: &Array) -> Real {
        self.cost_function
            .value(&self.projection.include(free_parameters))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Identity;

    impl CostFunction for Identity {
        fn values(&self, x: &Array) -> Array {
            x.clone()
        }
    }

    #[test]
    fn project_and_include_delegate_to_the_projection() {
        let cost = Identity;
        let pcf = ProjectedCostFunction::new(
            &cost,
            &Array::from([10.0, 20.0, 30.0]),
            vec![true, false, true],
        )
        .unwrap();
        assert_eq!(
            pcf.project(&Array::from([10.0, 20.0, 30.0])),
            Array::from([20.0])
        );
        assert_eq!(
            pcf.include(&Array::from([99.0])),
            Array::from([10.0, 99.0, 30.0])
        );
    }

    #[test]
    fn value_and_values_delegate_over_the_reassembled_vector() {
        let cost = Identity;
        let pcf = ProjectedCostFunction::new(
            &cost,
            &Array::from([10.0, 20.0, 30.0]),
            vec![true, false, true],
        )
        .unwrap();
        assert_eq!(
            pcf.values(&Array::from([99.0])),
            Array::from([10.0, 99.0, 30.0])
        );
        let expected = ((10.0 * 10.0 + 99.0 * 99.0 + 30.0 * 30.0) / 3.0_f64).sqrt();
        assert!((pcf.value(&Array::from([99.0])) - expected).abs() < 1e-12);
    }

    #[test]
    fn new_rejects_an_all_fixed_split() {
        let cost = Identity;
        let result = ProjectedCostFunction::new(&cost, &Array::from([1.0, 2.0]), vec![true, true]);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().message(), "numberOfFreeParameters==0");
    }
}
