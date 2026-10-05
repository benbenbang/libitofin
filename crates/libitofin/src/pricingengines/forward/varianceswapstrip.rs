use crate::errors::QlResult;
use crate::instruments::VarianceSwapOptionWeight;
use crate::option::OptionType;
use crate::require;

/// Maximum raw strike count on each side of a variance-swap replication strip.
pub const MAX_VARIANCE_SWAP_STRIKES: usize = 4096;

pub(super) struct VarianceSwapStrip {
    calls: Vec<f64>,
    puts: Vec<f64>,
    dk: f64,
}

impl VarianceSwapStrip {
    pub(super) fn new(dk: f64, calls: &[f64], puts: &[f64]) -> QlResult<Self> {
        require!(
            dk.is_finite() && dk > 0.0,
            "variance strip dk must be finite and positive"
        );
        for strikes in [calls, puts] {
            require!(
                (2..=MAX_VARIANCE_SWAP_STRIKES).contains(&strikes.len()),
                "variance strip requires 2..4096 raw strikes per side"
            );
            require!(
                strikes.iter().all(|k| k.is_finite() && *k > 0.0),
                "variance strip strikes must be finite and positive"
            );
        }
        let mut calls = calls.to_vec();
        let mut puts = puts.to_vec();
        calls.sort_by(f64::total_cmp);
        puts.sort_by(|a, b| b.total_cmp(a));
        calls.dedup();
        puts.dedup();
        require!(
            calls.len() >= 2 && puts.len() >= 2,
            "variance strip needs two distinct strikes per side"
        );
        require!(
            calls[0] == puts[0],
            "variance strip call/put boundary must match exactly"
        );
        let max_call = calls[calls.len() - 1];
        let min_put = puts[puts.len() - 1];
        require!(
            (max_call + dk).is_finite() && max_call + dk > max_call,
            "variance strip upper tail must be finite and increasing"
        );
        require!(
            min_put - dk > 0.0 && min_put - dk < min_put,
            "variance strip lower tail must be positive and decreasing"
        );
        Ok(Self { calls, puts, dk })
    }

    pub(super) fn boundary(&self) -> f64 {
        self.calls[0]
    }

    pub(super) fn weights(&self, time: f64) -> QlResult<Vec<VarianceSwapOptionWeight>> {
        require!(
            time.is_finite() && time > 0.0,
            "variance strip time must be finite and positive"
        );
        let mut weights = Vec::with_capacity(self.calls.len() + self.puts.len());
        for (option_type, strikes, tail) in [
            (
                OptionType::Call,
                &self.calls,
                self.calls[self.calls.len() - 1] + self.dk,
            ),
            (
                OptionType::Put,
                &self.puts,
                self.puts[self.puts.len() - 1] - self.dk,
            ),
        ] {
            let mut previous_slope = 0.0;
            for (index, &strike) in strikes.iter().enumerate() {
                let next = strikes.get(index + 1).copied().unwrap_or(tail);
                let payoff = self.payoff(strike, time)?;
                let next_payoff = self.payoff(next, time)?;
                let numerator = next_payoff - payoff;
                let denominator = next - strike;
                require!(
                    numerator.is_finite() && denominator.is_finite() && denominator != 0.0,
                    "nonfinite variance strip slope inputs"
                );
                let slope = (numerator / denominator).abs();
                let weight = slope - previous_slope;
                require!(
                    slope.is_finite() && weight.is_finite(),
                    "nonfinite variance strip weight"
                );
                weights.push(VarianceSwapOptionWeight {
                    option_type,
                    strike,
                    weight,
                });
                previous_slope = slope;
            }
        }
        Ok(weights)
    }

    fn payoff(&self, strike: f64, time: f64) -> QlResult<f64> {
        let boundary = self.boundary();
        let ratio = strike / boundary;
        let linear = (strike - boundary) / boundary;
        let scale = 2.0 / time;
        require!(
            ratio.is_finite() && ratio > 0.0 && linear.is_finite() && scale.is_finite(),
            "nonfinite variance log-payoff inputs"
        );
        let value = scale * (linear - ratio.ln());
        require!(value.is_finite(), "nonfinite variance log payoff");
        Ok(value)
    }
}
