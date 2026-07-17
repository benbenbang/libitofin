//! Fixed/free parameter split for constrained calibration.
//!
//! Port of `ql/math/optimization/projection.{hpp,cpp}`. A [`Projection`] holds
//! a seed parameter vector together with a boolean mask marking which entries
//! are held fixed. [`Projection::project`] extracts the free subset;
//! [`Projection::include`] rebuilds the full vector, reinstating the fixed
//! slots from the seed. `CalibratedModel::calibrate` uses this to optimize over
//! the free parameters only, then reassembles the full parameter set.
//!
//! QuantLib's protected `mapFreeParameters` and its `mutable actualParameters_`
//! cache are used only by `ProjectedCostFunction`, which is not part of this
//! wave; they are deferred until that class is ported. `project`/`include`
//! reconstruct their result directly, so nothing here depends on the cache.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::require;

/// A split of a parameter vector into fixed and free entries.
#[derive(Clone, Debug)]
pub struct Projection {
    number_of_free_parameters: usize,
    seed: Array,
    fix_parameters: Vec<bool>,
}

impl Projection {
    /// A projection of `parameter_values`, holding fixed every entry whose
    /// `fix_parameters` flag is `true`. An empty `fix_parameters` leaves every
    /// entry free.
    ///
    /// # Errors
    ///
    /// Fails if `fix_parameters` is non-empty and its length differs from the
    /// seed, or if every entry is fixed (no free parameters remain).
    pub fn new(parameter_values: &Array, fix_parameters: Vec<bool>) -> QlResult<Self> {
        let seed = parameter_values.clone();
        let fix_parameters = if fix_parameters.is_empty() {
            vec![false; seed.size()]
        } else {
            fix_parameters
        };
        require!(
            seed.size() == fix_parameters.len(),
            "fixedParameters_.size()!=parametersFreedoms_.size()"
        );
        let number_of_free_parameters = fix_parameters.iter().filter(|&&fixed| !fixed).count();
        require!(number_of_free_parameters > 0, "numberOfFreeParameters==0");
        Ok(Projection {
            number_of_free_parameters,
            seed,
            fix_parameters,
        })
    }

    /// Returns the subset of free parameters drawn from `parameters`.
    ///
    /// # Panics
    ///
    /// Panics if `parameters` is not the size of the seed.
    pub fn project(&self, parameters: &Array) -> Array {
        assert!(
            parameters.size() == self.fix_parameters.len(),
            "parameters.size()!=parametersFreedoms_.size()"
        );
        let mut projected = Array::with_size(self.number_of_free_parameters);
        let mut i = 0;
        for j in 0..self.fix_parameters.len() {
            if !self.fix_parameters[j] {
                projected[i] = parameters[j];
                i += 1;
            }
        }
        projected
    }

    /// Returns the full parameter vector, taking free entries from
    /// `projected_parameters` and reinstating the fixed entries from the seed.
    ///
    /// # Panics
    ///
    /// Panics if `projected_parameters` is not the number of free parameters.
    pub fn include(&self, projected_parameters: &Array) -> Array {
        assert!(
            projected_parameters.size() == self.number_of_free_parameters,
            "projectedParameters.size()!=numberOfFreeParameters"
        );
        let mut full = self.seed.clone();
        let mut i = 0;
        for j in 0..full.size() {
            if !self.fix_parameters[j] {
                full[j] = projected_parameters[i];
                i += 1;
            }
        }
        full
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_free_projection_is_the_identity() {
        let proj = Projection::new(&Array::from([1.0, 2.0, 3.0]), Vec::new()).unwrap();
        let x = Array::from([4.0, 5.0, 6.0]);
        assert_eq!(proj.project(&x), x);
        assert_eq!(proj.include(&x), x);
    }

    #[test]
    fn project_extracts_free_slots_and_include_reinstates_fixed_slots() {
        let proj =
            Projection::new(&Array::from([10.0, 20.0, 30.0]), vec![true, false, true]).unwrap();
        assert_eq!(
            proj.project(&Array::from([10.0, 20.0, 30.0])),
            Array::from([20.0])
        );
        assert_eq!(
            proj.include(&Array::from([99.0])),
            Array::from([10.0, 99.0, 30.0])
        );
    }

    #[test]
    fn new_rejects_mismatched_mask_length() {
        let err = Projection::new(&Array::from([1.0, 2.0]), vec![false]).unwrap_err();
        assert_eq!(
            err.message(),
            "fixedParameters_.size()!=parametersFreedoms_.size()"
        );
    }

    #[test]
    fn new_rejects_an_all_fixed_split() {
        let err = Projection::new(&Array::from([1.0, 2.0]), vec![true, true]).unwrap_err();
        assert_eq!(err.message(), "numberOfFreeParameters==0");
    }

    #[test]
    #[should_panic(expected = "parameters.size()!=parametersFreedoms_.size()")]
    fn project_rejects_wrong_sized_input() {
        let proj = Projection::new(&Array::from([1.0, 2.0, 3.0]), vec![true, false, true]).unwrap();
        let _ = proj.project(&Array::from([1.0, 2.0]));
    }

    #[test]
    #[should_panic(expected = "projectedParameters.size()!=numberOfFreeParameters")]
    fn include_rejects_wrong_sized_input() {
        let proj = Projection::new(&Array::from([1.0, 2.0, 3.0]), vec![true, false, true]).unwrap();
        let _ = proj.include(&Array::from([1.0, 2.0]));
    }
}
