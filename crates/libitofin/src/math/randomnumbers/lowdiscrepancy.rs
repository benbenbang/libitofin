//! Weighted Sobol adapter and generic low-discrepancy policy.

use super::inversecumulativersg::InverseCumulativeRsg;
use super::rngtraits::{InverseCumulative, McRngTraits, SequenceGenerator};
use super::sobol::{DirectionIntegers, PPMT_MAX_DIM, SobolRsg};
use crate::errors::QlResult;
use crate::math::distributions::normal::InverseCumulativeNormal;
use crate::methods::montecarlo::Sample;
use crate::require;
use crate::types::Real;
use std::marker::PhantomData;

/// A Sobol state exposed as weighted sequences with unit weight.
#[derive(Clone)]
pub struct SobolSequence {
    generator: SobolRsg,
    sample: Sample<Vec<Real>>,
}

impl SobolSequence {
    /// Own an existing Sobol state without advancing it.
    pub fn new(generator: SobolRsg) -> Self {
        let sample = Sample::new(vec![0.0; generator.dimension()], 1.0);
        Self { generator, sample }
    }
}

impl SequenceGenerator for SobolSequence {
    fn next_sequence(&mut self) -> &Sample<Vec<Real>> {
        self.sample
            .value
            .copy_from_slice(self.generator.next_sequence());
        &self.sample
    }
    fn last_sequence(&self) -> &Sample<Vec<Real>> {
        &self.sample
    }
    fn dimension(&self) -> usize {
        self.generator.dimension()
    }
}

/// A seeded factory for low-discrepancy uniform sequences strictly in `(0, 1)`.
pub trait LowDiscrepancySequence: SequenceGenerator {
    /// Maximum valid number of draws from a fresh generator.
    const MAX_SAMPLES: Option<usize> = None;
    /// Construct a sequence, rejecting unsupported dimensions.
    fn with_seed(dimension: usize, seed: u32) -> QlResult<Self>
    where
        Self: Sized;
}

impl LowDiscrepancySequence for SobolSequence {
    const MAX_SAMPLES: Option<usize> = Some(u32::MAX as usize);
    fn with_seed(dimension: usize, seed: u32) -> QlResult<Self> {
        require!(
            (1..=PPMT_MAX_DIM).contains(&dimension),
            "unsupported Sobol dimension {dimension}"
        );
        Ok(Self::new(SobolRsg::new(
            dimension,
            u64::from(seed),
            DirectionIntegers::Jaeckel,
        )))
    }
}

/// An inverse transform with an explicit default factory.
pub trait DefaultInverseCumulative: InverseCumulative {
    /// Construct the default distribution transform.
    fn default_inverse() -> Self;
}

impl DefaultInverseCumulative for InverseCumulativeNormal {
    fn default_inverse() -> Self {
        Self::standard()
    }
}

/// Generic low-discrepancy policy; deterministic quadrature has no MC error estimate.
pub struct GenericLowDiscrepancy<USG, IC>(PhantomData<(USG, IC)>);

impl<USG: LowDiscrepancySequence, IC: InverseCumulative> GenericLowDiscrepancy<USG, IC> {
    /// Construct with a caller-owned transform instead of global mutable state.
    pub fn with_inverse(
        dimension: usize,
        seed: u32,
        inverse: IC,
    ) -> QlResult<InverseCumulativeRsg<USG, IC>> {
        Ok(InverseCumulativeRsg::new(
            USG::with_seed(dimension, seed)?,
            inverse,
        ))
    }
}

impl<USG: LowDiscrepancySequence, IC: DefaultInverseCumulative> McRngTraits
    for GenericLowDiscrepancy<USG, IC>
{
    type RsgType = InverseCumulativeRsg<USG, IC>;
    const ALLOWS_ERROR_ESTIMATE: bool = false;
    const MAX_SAMPLES: Option<usize> = USG::MAX_SAMPLES;
    fn make_sequence_generator(dimension: usize, seed: u32) -> QlResult<Self::RsgType> {
        Self::with_inverse(dimension, seed, IC::default_inverse())
    }
}

/// QuantLib's default Sobol/Jaeckel standard-normal low-discrepancy policy.
pub type LowDiscrepancy = GenericLowDiscrepancy<SobolSequence, InverseCumulativeNormal>;
