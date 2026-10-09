//! Checked Householder vector transformations and direction-based reflections.
//!
//! QuantLib's application uses `I - 2 v vᵀ`, whereas its explicit matrix
//! normalizes `v` first. These are deliberately different for non-unit vectors.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::require;
use crate::types::Real;

/// Immutable transformation with QuantLib's unnormalized application law.
#[derive(Clone, Debug)]
pub struct HouseholderTransformation {
    v: Array,
}

impl HouseholderTransformation {
    /// Stores a nonempty finite vector without silently normalizing it.
    ///
    /// # Errors
    /// Rejects empty or nonfinite vectors. Zero vectors are allowed.
    pub fn new(v: Array) -> QlResult<Self> {
        validate(&v)?;
        Ok(Self { v })
    }

    /// Applies `x - 2 (v · x) v`, not the normalized matrix law.
    ///
    /// Scaling avoids overflow in the input's norm and inner product. Ordinary
    /// floating-point rounding and underflow still apply.
    ///
    /// # Errors
    /// Rejects dimension mismatches, nonfinite inputs and nonfinite intermediate
    /// coefficients or outputs, even if exact arithmetic could cancel them.
    pub fn apply(&self, x: &Array) -> QlResult<Array> {
        validate(x)?;
        require!(x.size() == self.v.size(), "Householder dimension mismatch");
        let scale = max_abs(x);
        if scale == 0.0 {
            return Ok(x.clone());
        }
        let scaled: Array = x.iter().map(|value| value / scale).collect();
        let dot = self
            .v
            .iter()
            .zip(scaled.iter())
            .map(|(v, x)| v * x)
            .sum::<Real>();
        require!(dot.is_finite(), "Householder inner product overflowed");
        if dot == 0.0 {
            return Ok(x.clone());
        }
        let coefficient = 2.0 * dot;
        require!(
            coefficient.is_finite(),
            "Householder coefficient overflowed"
        );
        self.v
            .iter()
            .zip(scaled.iter())
            .map(|(v, x)| {
                let value = (x - coefficient * v) * scale;
                require!(value.is_finite(), "Householder output overflowed");
                Ok(value)
            })
            .collect()
    }

    /// Returns `I - 2 y yᵀ`, where `y = v / ||v||`.
    ///
    /// Unlike [`Self::apply`], this normalizes non-unit vectors. A zero vector's
    /// checked matrix is identity, extending the native undefined `0 / 0` case.
    ///
    /// # Errors
    /// Rejects dimensions whose square exceeds the addressable matrix size.
    pub fn matrix(&self) -> QlResult<Matrix> {
        let n = self.v.size();
        require!(
            n.checked_mul(n)
                .is_some_and(|len| len <= isize::MAX as usize / size_of::<Real>()),
            "Householder matrix is too large"
        );
        let scale = max_abs(&self.v);
        let mut matrix = Matrix::with_size(n, n);
        if scale == 0.0 {
            for i in 0..n {
                matrix[(i, i)] = 1.0;
            }
            return Ok(matrix);
        }
        let scaled: Array = self.v.iter().map(|v| v / scale).collect();
        let norm_squared = scaled.dot(&scaled);
        for i in 0..n {
            for j in 0..=i {
                let value =
                    if i == j { 1.0 } else { 0.0 } - 2.0 * scaled[i] * scaled[j] / norm_squared;
                matrix[(i, j)] = value;
                matrix[(j, i)] = value;
            }
        }
        Ok(matrix)
    }
}

/// Builds a reflection vector relative to a supplied unit direction.
///
/// The native tiny-angle branch preserves the sign of the projection onto the
/// direction. It is not equivalent to always mapping a vector to `+||a|| e`.
#[derive(Clone, Debug)]
pub struct HouseholderReflection {
    e: Array,
}

impl HouseholderReflection {
    /// Stores a unit direction without normalizing it.
    ///
    /// # Errors
    /// Rejects empty/nonfinite directions or norms farther than `32 EPSILON`
    /// from one. This checks the native caller's unit-direction precondition.
    pub fn new(e: Array) -> QlResult<Self> {
        validate(&e)?;
        let is_unit = (norm(&e) - 1.0).abs() <= 32.0 * Real::EPSILON;
        require!(is_unit, "Householder direction must have unit norm");
        Ok(Self { e })
    }

    /// Returns QuantLib's reflection vector, with input rescaling.
    ///
    /// Parallel and antiparallel vectors return zero. Very small angles use
    /// QuantLib's fourth-order branch; other angles use `a - ||a|| e`.
    ///
    /// # Errors
    /// Rejects zero, nonfinite or wrong-dimensional inputs and nonfinite results.
    pub fn reflection_vector(&self, a: &Array) -> QlResult<Array> {
        validate(a)?;
        require!(a.size() == self.e.size(), "Householder dimension mismatch");
        let scale = max_abs(a);
        require!(scale != 0.0, "vector of length zero given");
        let a: Array = a.iter().map(|value| value / scale).collect();
        let dot = a.dot(&self.e);
        let a1 = &self.e * dot;
        let a2 = &a - &a1;
        let ratio = norm(&a2) / dot.abs();
        let eps = ratio * ratio;
        let vector = if eps < Real::EPSILON * Real::EPSILON {
            Array::with_size(a.size())
        } else if eps < 1e-4 {
            let eps2 = eps * eps;
            let eps3 = eps * eps2;
            let eps4 = eps2 * eps2;
            let numerator =
                &a2 - &(&a1 * (eps / 2.0 - eps2 / 8.0 + eps3 / 16.0 - 5.0 / 128.0 * eps4));
            let denominator = dot * (eps + eps2 / 4.0 - eps3 / 8.0 + 5.0 / 64.0 * eps4).sqrt();
            &numerator / denominator
        } else {
            let c = &a - &(&self.e * norm(&a));
            &c / norm(&c)
        };
        require!(
            vector.iter().all(|value| value.is_finite()),
            "Householder reflection vector is nonfinite"
        );
        Ok(vector)
    }

    /// Applies the reflection to `a`, preserving the native branch convention.
    ///
    /// # Errors
    /// Propagates [`Self::reflection_vector`] and transformation application errors.
    pub fn apply(&self, a: &Array) -> QlResult<Array> {
        HouseholderTransformation::new(self.reflection_vector(a)?)?.apply(a)
    }
}

/// Returns the normalized matrix for a direction's reflection vector.
///
/// Parallel and antiparallel inputs produce identity, instead of native NaNs.
///
/// # Errors
/// Propagates checked direction, vector and matrix-construction errors.
pub fn householder_transformation(e: Array, target: &Array) -> QlResult<Matrix> {
    HouseholderTransformation::new(HouseholderReflection::new(e)?.reflection_vector(target)?)?
        .matrix()
}

fn validate(values: &Array) -> QlResult<()> {
    require!(!values.is_empty(), "Householder vector must not be empty");
    require!(
        values.iter().all(|value| value.is_finite()),
        "Householder vector must be finite"
    );
    Ok(())
}

fn max_abs(values: &Array) -> Real {
    values
        .iter()
        .fold(0.0, |scale, value| scale.max(value.abs()))
}

fn norm(values: &Array) -> Real {
    values.iter().fold(0.0, |norm, value| norm.hypot(*value))
}
