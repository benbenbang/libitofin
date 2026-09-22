//! Cubic smile used by the stripped-optionlet adapter.
use crate::errors::QlResult;
use crate::math::interpolations::Interpolation;
use crate::math::interpolations::cubic::{
    CubicBoundaryCondition, CubicDerivativeApprox, CubicInterpolation,
};
use crate::termstructures::volatility::{SmileSection, SmileSectionBase, VolatilityType};
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::types::{Rate, Real, Time, Volatility};

pub(super) struct OptionletCubicSmile {
    base: SmileSectionBase,
    interpolation: CubicInterpolation,
}

impl OptionletCubicSmile {
    pub(super) fn new(
        time: Time,
        strikes: Vec<Rate>,
        vols: Vec<Volatility>,
        kind: VolatilityType,
        shift: Real,
    ) -> QlResult<Self> {
        crate::require!(
            time.is_finite() && time > 0.0,
            "smile exercise time must be positive"
        );
        let boundary = if strikes.len() >= 4 {
            CubicBoundaryCondition::Lagrange
        } else {
            CubicBoundaryCondition::SecondDerivative
        };
        let interpolation = CubicInterpolation::new(strikes, vols, CubicDerivativeApprox::Spline)?
            .with_boundary_conditions(boundary, 0.0, boundary, 0.0)?
            .with_extrapolation(true);
        Ok(Self {
            base: SmileSectionBase::with_exercise_time(time, Actual365Fixed::new(), kind, shift)?,
            interpolation,
        })
    }
}

impl SmileSection for OptionletCubicSmile {
    fn base(&self) -> &SmileSectionBase {
        &self.base
    }
    fn min_strike(&self) -> Rate {
        self.interpolation.x_min()
    }
    fn max_strike(&self) -> Rate {
        self.interpolation.x_max()
    }
    fn atm_level(&self) -> Option<Rate> {
        None
    }
    fn volatility_impl(&self, strike: Rate) -> QlResult<Volatility> {
        crate::require!(strike.is_finite(), "smile strike must be finite");
        let vol = self.interpolation.value(strike)?;
        crate::require!(
            vol.is_finite(),
            "interpolated smile volatility must be finite"
        );
        Ok(vol.max(0.0))
    }
}
