use std::cell::RefCell;

use crate::errors::{QlError, QlResult};
use crate::math::integrals::Integrator;
use crate::math::integrals::segment::SegmentIntegral;
use crate::methods::montecarlo::{Path, PathPricer};
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess1D;
use crate::{fail, require};

pub(super) struct VariancePathPricer {
    pub process: Shared<GeneralizedBlackScholesProcess>,
    pub error: RefCell<Option<QlError>>,
}

impl VariancePathPricer {
    fn variance(&self, path: &Path) -> QlResult<f64> {
        require!(path.length() >= 2, "variance MC path is empty");
        require!(
            path.values().iter().all(|x| x.is_finite() && *x > 0.0),
            "variance MC path states must be finite and positive"
        );
        let grid = path.time_grid();
        let Some(end) = grid.back() else {
            fail!("variance MC path end missing");
        };
        let dt = grid.dt(0);
        require!(
            end.is_finite() && end > 0.0 && dt.is_finite() && dt > 0.0,
            "variance MC grid must have positive finite spacing"
        );
        let count = end / dt;
        require!(
            count.is_finite() && count >= 1.0 && count <= path.length() as f64,
            "variance MC integration count invalid"
        );
        let intervals = count as usize;
        let integrator = SegmentIntegral::new(intervals)?;
        let mut failure = None;
        let integral = integrator.integrate(
            |time| {
                let result = (|| {
                    let index = time / dt;
                    require!(
                        index.is_finite() && index >= 0.0 && index < path.length() as f64,
                        "variance MC integration index invalid"
                    );
                    let Some(state) = path.at(index as usize) else {
                        fail!("variance MC integration state missing");
                    };
                    let sigma = self.process.diffusion(time, state)?;
                    let variance = sigma * sigma;
                    require!(
                        sigma.is_finite() && sigma >= 0.0 && variance.is_finite(),
                        "variance MC diffusion must be finite and nonnegative"
                    );
                    Ok(variance)
                })();
                match result {
                    Ok(value) => value,
                    Err(error) => {
                        if failure.is_none() {
                            failure = Some(error);
                        }
                        f64::NAN
                    }
                }
            },
            0.0,
            end,
        )?;
        if let Some(error) = failure {
            return Err(error);
        }
        let variance = integral / end;
        require!(
            integral.is_finite() && variance.is_finite() && variance >= 0.0,
            "variance MC integrated variance must be finite and nonnegative"
        );
        Ok(variance)
    }
}

impl PathPricer<Path> for VariancePathPricer {
    fn price(&self, path: &Path) -> f64 {
        match self.variance(path) {
            Ok(value) => value,
            Err(error) => {
                let mut failure = self.error.borrow_mut();
                if failure.is_none() {
                    *failure = Some(error);
                }
                f64::NAN
            }
        }
    }
}
