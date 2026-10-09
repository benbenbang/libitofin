# Householder utilities

These additive Rust-only utilities do not replace QR, SVD or existing matrix
decompositions. No binding or existing default changes are included.

```rust
use libitofin::math::array::Array;
use libitofin::math::matrixutilities::{HouseholderReflection, householder_transformation};

let direction = Array::from([1.0, 0.0, 0.0]);
let target = Array::from([3.0, 4.0, 0.0]);
let reflection = HouseholderReflection::new(direction.clone())?;
let reflected = reflection.apply(&target)?;
let matrix = householder_transformation(direction, &target)?;
# Ok::<(), libitofin::errors::QlError>(())
```

## Two distinct transformation laws

| API | Law | Contract |
| --- | --- | --- |
| `HouseholderTransformation::new(v)` | Stores `v` unchanged | Finite, nonempty; zero is allowed |
| `apply(x)` | `x - 2 (v · x) v` | Does **not** normalize `v` |
| `matrix()` | `I - 2 y yᵀ`, with `y = v / ||v||` | Normalizes nonzero `v` |
| `HouseholderReflection::new(e)` | Stores direction `e` unchanged | Unit norm within `32 f64::EPSILON` |
| `reflection_vector(a)` | QuantLib's three-branch formula | Nonzero `a`, matching dimension |
| `householder_transformation(e, a)` | Matrix from the reflection vector | Same checked inputs |

The distinction between `apply` and `matrix` is intentional upstream behavior.
For example, with `v = [2, 0]` and `x = [3, 4]`, application gives `[-21, 4]`
whereas the normalized matrix gives `[-3, 4]`. Do not describe arbitrary
non-unit application as an orthogonal reflection.

## Reflection branch conventions

- For an ordinary angle, the reflection vector is the normalized
  `a - ||a|| e`, mapping toward the positive direction.
- For a squared tangent below `1e-4`, the original fourth-order expression
  avoids cancellation. A **negative projection remains negative** in this branch.
- Below `f64::EPSILON²`, the reflection vector is zero and application is
  identity. Parallel and antiparallel vectors therefore remain unchanged.
- Zero reflection vectors return an **identity matrix** in the checked Rust API.
  The original C++ explicit matrix instead divides by zero and produces NaNs.
  This defined extension also makes the convenience helper usable for parallel
  vectors. Zero target vectors remain an error.

## Numerical and error boundaries

- All constructors and calculations return `QlResult` for invalid input.
- Nonfinite components, empty vectors and incompatible dimensions are rejected.
- Input rescaling and `hypot` norms avoid raw squared-norm overflow/underflow.
  Finite large and tiny vectors are supported without requiring their physical
  norm to fit in `f64`.
- Final unrepresentable values and nonfinite application coefficients are
  rejected. A non-unit transformation can fail on intermediate overflow even
  when exact arithmetic could cancel; arbitrary-precision arithmetic is not
  promised.
- Ordinary binary64 rounding and underflow remain. Mixed-scale components can
  disappear during rescaling, and components below the tiny-angle threshold are
  intentionally left unchanged. No bit-for-bit promise applies to rescaled
  computations.
- Matrix dimensions are checked for addressable size. Allocation exhaustion is
  not converted into a numerical error, consistent with existing `Matrix` APIs.

## Oracle provenance

The fixture generator compiles the **original** `HouseholderReflection` and
`HouseholderTransformation` implementation at QuantLib source revision
`9863b578af0caa4cecabf697196533e84a8308b6`. It does not reimplement the formulas
as its oracle. The separately installed QuantLib 1.43 Python wheel does not
expose these classes. Eleven original compiled cases cover all branches,
projection signs, parallel/orthogonal targets and a non-axis unit direction.
Additional Rust tests cover symmetry, orthogonality, involution, non-unit laws,
zero-vector matrix policy, large/tiny scales and checked failures. These tests
are not claims of a decomposition migration or performance improvement.
