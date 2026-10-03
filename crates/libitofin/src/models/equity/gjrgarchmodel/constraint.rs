use crate::math::array::Array;
use crate::math::optimization::constraint::Constraint;
use crate::processes::GjrGarchParameters;
use crate::types::Real;

pub(super) struct ScalarDomain {
    index: usize,
}

impl ScalarDomain {
    pub(super) fn new(index: usize) -> Self {
        Self { index }
    }

    fn accepts(&self, value: Real) -> bool {
        let (lower, upper) = self.bounds();
        value.is_finite()
            && value >= lower
            && value <= upper
            && (!matches!(self.index, 0 | 5) || value > 0.0)
    }

    fn bounds(&self) -> (Real, Real) {
        match self.index {
            0 | 5 => (0.0, Real::MAX),
            1 | 2 => (0.0, 1.0),
            3 => (-1.0, 1.0),
            _ => (-Real::MAX, Real::MAX),
        }
    }
}

impl Constraint for ScalarDomain {
    fn test(&self, params: &Array) -> bool {
        params.iter().all(|&value| self.accepts(value))
    }

    fn lower_bound(&self, params: &Array) -> Array {
        Array::filled(params.size(), self.bounds().0)
    }

    fn upper_bound(&self, params: &Array) -> Array {
        Array::filled(params.size(), self.bounds().1)
    }
}

pub(super) struct GjrGarchConstraint {
    days_per_year: Real,
}

impl GjrGarchConstraint {
    pub(super) fn new(days_per_year: Real) -> Self {
        Self { days_per_year }
    }
}

impl Constraint for GjrGarchConstraint {
    fn test(&self, params: &Array) -> bool {
        params.size() == 6
            && params
                .iter()
                .enumerate()
                .all(|(index, &value)| ScalarDomain::new(index).accepts(value))
            && params[2] + params[3] >= 0.0
            && GjrGarchParameters {
                omega: params[0],
                alpha: params[1],
                beta: params[2],
                gamma: params[3],
                lambda: params[4],
                v0: params[5],
                days_per_year: self.days_per_year,
            }
            .validate()
            .is_ok()
    }

    fn lower_bound(&self, params: &Array) -> Array {
        (0..params.size())
            .map(|index| ScalarDomain::new(index).bounds().0)
            .collect()
    }

    fn upper_bound(&self, params: &Array) -> Array {
        (0..params.size())
            .map(|index| ScalarDomain::new(index).bounds().1)
            .collect()
    }
}
