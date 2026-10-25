//! Immutable bilinear CPI cap/floor quoted-price surface.
//!
//! Adapted from QuantLib revision 9863b578af0caa4cecabf697196533e84a8308b6,
//! ql/experimental/inflation/cpicapfloortermpricesurface.{hpp,cpp}.
//! Copyright (C) 2010 Chris Kenyon. Distributed under the QuantLib license.
//! This bounded implementation freezes the reference date as well as the parity
//! grids. Rebuild after date, fixing or curve changes. Only whole-year tenors
//! are accepted, because the native parity calculation uses tenor.length().

use crate::cashflows::cpicoupon::{checked_cpi_offset, checked_cpi_shift, observed_cpi};
use crate::cashflows::cpidates::{checked_cpi_adjust as adjusted, valid_cpi_date as valid_date};
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::inflationindex::{
    CpiInterpolationType, InflationIndex, ZeroInflationIndex, inflation_year_fraction,
};
use crate::instruments::cpicapfloor::lag_months;
use crate::math::comparison::close_enough;
use crate::math::interpolations::Interpolation2D;
use crate::math::interpolations::bilinear::BilinearInterpolation;
use crate::math::matrix::Matrix;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::{
    businessdayconvention::BusinessDayConvention, calendar::Calendar, date::Date,
    daycounter::DayCounter, period::Period, timeunit::TimeUnit,
};
use crate::{fail, require};

/// Inputs to an immutable CPI quoted-price snapshot. Matrices are strike rows
/// by maturity columns. Nominal is metadata only, matching native parity units.
pub struct CpiCapFloorSurfaceConfig {
    pub nominal: f64,
    pub base_rate: f64,
    pub observation_lag: Period,
    pub calendar: Calendar,
    pub business_day_convention: BusinessDayConvention,
    pub day_counter: DayCounter,
    pub index: Shared<ZeroInflationIndex>,
    pub interpolation: CpiInterpolationType,
    pub discount: Handle<dyn YieldTermStructure>,
    pub cap_strikes: Vec<f64>,
    pub floor_strikes: Vec<f64>,
    pub maturities: Vec<Period>,
    pub cap_prices: Matrix,
    pub floor_prices: Matrix,
    pub settings: Shared<Settings<Date>>,
}

/// A frozen price grid with bilinear extrapolation enabled, including negative
/// extrapolated values. There is no volatility model or notional scaling.
pub struct CpiCapFloorTermPriceSurface {
    reference_date: Date,
    day_counter: DayCounter,
    observation_lag: Period,
    nominal: f64,
    cap: BilinearInterpolation,
    floor: BilinearInterpolation,
}

impl CpiCapFloorTermPriceSurface {
    /// Builds parity-completed grids at the current evaluation date.
    ///
    /// # Errors
    /// Rejects missing curves/fixings, mixed settings, non-finite inputs,
    /// nonpositive nominal/prices, invalid grids, nonyear tenors and negative lag.
    pub fn new(a: CpiCapFloorSurfaceConfig) -> QlResult<Self> {
        require!(
            a.nominal.is_finite() && a.nominal > 0.0,
            "nominal must be positive and finite"
        );
        require!(
            a.base_rate.is_finite() && a.base_rate > -1.0,
            "base rate must be finite and greater than -1"
        );
        lag_months(a.observation_lag)?;
        require!(
            Shared::ptr_eq(a.index.inflation_base().settings(), &a.settings),
            "index and surface must share settings"
        );
        require!(
            !a.index.term_structure().is_empty(),
            "ZITS missing from index"
        );
        let discount = a.discount.current_link()?;
        let Some(reference_date) = a.settings.evaluation_date() else {
            fail!("evaluation date not set")
        };
        let reference_date = adjusted(
            &a.calendar,
            reference_date,
            BusinessDayConvention::Following,
        )?;
        require!(
            a.cap_strikes.len() > 1 && a.floor_strikes.len() > 1 && a.maturities.len() > 1,
            "not enough strikes or maturities"
        );
        for strikes in [&a.cap_strikes, &a.floor_strikes] {
            require!(
                strikes.iter().all(|k| k.is_finite() && *k > -1.0),
                "strikes must be finite and greater than -1"
            );
            require!(
                strikes.windows(2).all(|p| p[1] > p[0]),
                "strikes not increasing"
            );
        }
        require!(
            a.maturities
                .iter()
                .all(|p| p.units() == TimeUnit::Years && p.length() > 0),
            "maturities must be positive whole-year tenors"
        );
        require!(
            a.maturities
                .windows(2)
                .all(|p| p[1].length() > p[0].length()),
            "maturities not increasing"
        );
        for (grid, strikes, cap) in [
            (&a.cap_prices, &a.cap_strikes, true),
            (&a.floor_prices, &a.floor_strikes, false),
        ] {
            require!(
                grid.rows() == strikes.len() && grid.columns() == a.maturities.len(),
                "price matrix dimensions mismatch"
            );
            for j in 0..grid.columns() {
                for i in 0..grid.rows() {
                    let value = grid[(i, j)];
                    require!(
                        value.is_finite() && value > 0.0,
                        "prices must be positive and finite"
                    );
                    if i > 0 {
                        require!(
                            if cap {
                                value <= grid[(i - 1, j)]
                            } else {
                                value >= grid[(i - 1, j)]
                            },
                            "prices not monotonic in strike"
                        );
                    }
                }
            }
        }
        let mut strikes = a.floor_strikes.clone();
        let last_floor = a.floor_strikes[a.floor_strikes.len() - 1];
        strikes.extend(
            a.cap_strikes
                .iter()
                .copied()
                .filter(|k| *k > last_floor + 0.000_000_1),
        );
        require!(
            strikes.len() > 2 && strikes.windows(2).all(|p| p[1] > p[0]),
            "invalid strike union"
        );
        let mut cap = vec![vec![0.0; a.maturities.len()]; strikes.len()];
        let mut floor = cap.clone();
        let mut times = Vec::with_capacity(a.maturities.len());
        let f0 = observed_cpi(&a.index, reference_date, a.observation_lag, a.interpolation)?;
        require!(f0.is_finite() && f0 > 0.0, "invalid base CPI fixing");
        for (j, tenor) in a.maturities.iter().enumerate() {
            let date = adjusted(
                &a.calendar,
                checked_cpi_offset(reference_date, *tenor, true)?,
                a.business_day_convention,
            )?;
            let df = discount.discount_date(date, false)?;
            let f1 = observed_cpi(&a.index, date, a.observation_lag, a.interpolation)?;
            require!(f1.is_finite() && f1 > 0.0, "invalid terminal CPI fixing");
            let t = inflation_year_fraction(
                a.index.frequency(),
                a.interpolation == CpiInterpolationType::Linear,
                &a.day_counter,
                checked_cpi_shift(reference_date, a.observation_lag)?,
                checked_cpi_shift(date, a.observation_lag)?,
            )?;
            let atm = if t > 0.0 {
                (f1 / f0).powf(1.0 / t) - 1.0
            } else {
                a.base_rate
            };
            require!(
                df.is_finite() && df > 0.0 && atm.is_finite() && atm > -1.0,
                "invalid discount or ATM inflation"
            );
            let forward = (1.0 + atm).powi(tenor.length()) * df;
            require!(forward.is_finite(), "non-finite parity forward");
            times.push(a.day_counter.year_fraction(reference_date, date));
            for (i, k) in strikes.iter().enumerate() {
                let strike = (1.0 + k).powi(tenor.length()) * df;
                require!(strike.is_finite(), "non-finite compounded strike");
                let fi = a.floor_strikes.iter().position(|x| close_enough(*x, *k));
                let ci = a.cap_strikes.iter().position(|x| close_enough(*x, *k));
                match (ci, fi) {
                    (Some(c), Some(f)) => {
                        cap[i][j] = a.cap_prices[(c, j)];
                        floor[i][j] = a.floor_prices[(f, j)];
                    }
                    (Some(c), None) => {
                        cap[i][j] = a.cap_prices[(c, j)];
                        floor[i][j] = cap[i][j] + strike - forward;
                    }
                    (None, Some(f)) => {
                        floor[i][j] = a.floor_prices[(f, j)];
                        cap[i][j] = floor[i][j] + forward - strike;
                    }
                    (None, None) => fail!("unfilled price grid cell"),
                }
            }
        }
        Ok(Self {
            reference_date,
            day_counter: a.day_counter,
            observation_lag: a.observation_lag,
            nominal: a.nominal,
            cap: BilinearInterpolation::new(times.clone(), strikes.clone(), cap)?
                .with_extrapolation(true),
            floor: BilinearInterpolation::new(times, strikes, floor)?.with_extrapolation(true),
        })
    }

    /// Frozen reference date; rebuilding is required after an evaluation-date roll.
    pub fn reference_date(&self) -> Date {
        self.reference_date
    }
    /// Lag of the quoted calibration instruments.
    pub fn observation_lag(&self) -> Period {
        self.observation_lag
    }
    /// Descriptive nominal, not an automatic price multiplier.
    pub fn nominal(&self) -> f64 {
        self.nominal
    }
    /// Quoted cap price with unrestricted bilinear extrapolation.
    pub fn cap_price(&self, date: Date, strike: f64) -> QlResult<f64> {
        self.read(&self.cap, date, strike)
    }
    /// Quoted floor price with unrestricted bilinear extrapolation.
    pub fn floor_price(&self, date: Date, strike: f64) -> QlResult<f64> {
        self.read(&self.floor, date, strike)
    }
    fn read(&self, grid: &BilinearInterpolation, date: Date, strike: f64) -> QlResult<f64> {
        valid_date(date)?;
        require!(strike.is_finite(), "strike must be finite");
        let value = grid.value(
            self.day_counter.year_fraction(self.reference_date, date),
            strike,
        )?;
        require!(value.is_finite(), "non-finite surface price");
        Ok(value)
    }
}
