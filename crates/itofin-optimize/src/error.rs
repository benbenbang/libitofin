/// A rejected input. One variant per validation rule, so callers match rather
/// than parse a message.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidInput {
    /// `x0` has no coordinates.
    #[error("x0 must not be empty")]
    EmptyX0,
    /// A starting coordinate is infinite or `NaN`.
    #[error("x0[{index}] is not finite")]
    NonfiniteX0 { index: usize },
    /// A bounds vector does not carry one entry per coordinate of `x0`.
    #[error("bounds length {found} does not match x0 length {expected}")]
    BoundsLength { expected: usize, found: usize },
    /// A bound is `NaN`. Infinities are legal and leave that side open.
    #[error("the bound at index {index} is NaN")]
    NanBound { index: usize },
    /// A lower bound exceeds its upper bound.
    #[error("the lower bound at index {index} exceeds the upper bound")]
    BoundsOrder { index: usize },
    /// A bound pair admits no finite coordinate: a `+inf` lower bound or a
    /// `-inf` upper bound.
    #[error("the bounds at index {index} admit no finite coordinate")]
    InfeasibleBound { index: usize },
    /// An option that must be positive when given is not.
    #[error("{option} must be positive")]
    NotPositive { option: &'static str },
    /// The chosen method cannot honour an option that was supplied.
    #[error("method {method} does not support {option}")]
    Unsupported {
        method: &'static str,
        option: &'static str,
    },
    /// A tolerance that must be finite and non-negative is not. Zero is legal,
    /// which is why this is not [`InvalidInput::NotPositive`].
    #[error("{option} must be finite and non-negative")]
    NotFiniteNonnegative { option: &'static str },
    /// An initial simplex does not carry one more point than there are
    /// coordinates.
    #[error("the initial simplex has {found} points, not the {expected} required")]
    SimplexPointCount { expected: usize, found: usize },
    /// A point of an initial simplex does not carry one coordinate per
    /// dimension.
    #[error("initial simplex point {point} has length {found}, not {expected}")]
    SimplexPointLength {
        point: usize,
        expected: usize,
        found: usize,
    },
    /// A coordinate of an initial simplex is infinite or `NaN`.
    #[error("initial simplex point {point} is not finite at index {index}")]
    NonfiniteSimplex { point: usize, index: usize },
}

/// Everything a run can fail with. Neither variant is a [`Termination`](crate::Termination).
#[derive(Debug, thiserror::Error)]
pub enum MinimizeError<E: std::error::Error + 'static> {
    /// The problem, the options or the method combination was rejected before
    /// any evaluation.
    #[error("invalid input: {0}")]
    InvalidInput(#[from] InvalidInput),
    /// The objective, its gradient or its callback failed. The error is carried
    /// by value and reaches the caller unchanged.
    #[error("objective failed")]
    Objective(#[source] E),
}
