//! Black/Bachelier rate-option engine factories, retaining observable quotes.
use crate::boundary::*;
use crate::rates_api::{curve, finite, settings};
use crate::time_api::day_counter;
use libitofin::handle::Handle;
use libitofin::pricingengines::swaption::CashAnnuityModel;
use libitofin::pricingengines::{
    BachelierSwaptionEngine, BlackCapFloorEngine, BlackSwaptionEngine,
};
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::termstructures::volatility::OptionletVolatilityStructure;
use libitofin::termstructures::volatility::SwaptionVolatilityStructure;
use libitofin::types::Real;

#[repr(C)]
pub struct ItofinRateEngineConfig {
    /// 0 Black swaption, 1 Bachelier swaption, 2 Black cap/floor.
    pub kind: i32,
    pub discount: u64,
    pub volatility: u64,
    pub settings: u64,
    pub day_counter: u64,
    pub displacement: Real,
    pub cash_annuity_model: i32,
    /// 0 volatility structure, 1 quote + day counter.
    pub flat: u8,
    /// Used only for cap/floor volatility-structure constructors.
    pub has_displacement: u8,
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_rate_engine_new(
    ctx: *mut Context,
    a: ItofinRateEngineConfig,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(out)?;
            if a.flat > 1 || a.has_displacement > 1 {
                return Err(BindingError::invalid("invalid rate engine flag"));
            }
            let discount = curve(c, a.discount)?;
            let model = match a.cash_annuity_model {
                0 => CashAnnuityModel::SwapRate,
                1 => CashAnnuityModel::DiscountCurve,
                _ => return Err(BindingError::invalid("invalid annuity model")),
            };
            let id = match a.kind {
                0 => {
                    let engine = if a.flat == 1 {
                        BlackSwaptionEngine::with_flat_vol(
                            discount,
                            crate::market_api::quote(c, a.volatility)?,
                            day_counter(c, a.day_counter)?,
                            finite(a.displacement)?,
                            model,
                            settings(c, a.settings)?,
                        )
                    } else {
                        BlackSwaptionEngine::new(
                            discount,
                            c.get::<Handle<dyn SwaptionVolatilityStructure>>(a.volatility)?,
                            model,
                            settings(c, a.settings)?,
                        )
                    };
                    c.insert(shared_mut(engine))?
                }
                1 => {
                    let engine = if a.flat == 1 {
                        BachelierSwaptionEngine::with_flat_vol(
                            discount,
                            crate::market_api::quote(c, a.volatility)?,
                            day_counter(c, a.day_counter)?,
                            finite(a.displacement)?,
                            model,
                            settings(c, a.settings)?,
                        )
                    } else {
                        BachelierSwaptionEngine::new(
                            discount,
                            c.get::<Handle<dyn SwaptionVolatilityStructure>>(a.volatility)?,
                            model,
                            settings(c, a.settings)?,
                        )
                    };
                    c.insert(shared_mut(engine))?
                }
                2 => {
                    let engine = if a.flat == 1 {
                        BlackCapFloorEngine::with_flat_vol(
                            discount,
                            crate::market_api::quote(c, a.volatility)?,
                            day_counter(c, a.day_counter)?,
                            finite(a.displacement)?,
                            settings(c, a.settings)?,
                        )?
                    } else {
                        BlackCapFloorEngine::new(
                            discount,
                            c.get::<Handle<dyn OptionletVolatilityStructure>>(a.volatility)?,
                            if a.has_displacement == 1 {
                                Some(finite(a.displacement)?)
                            } else {
                                None
                            },
                        )?
                    };
                    c.insert(shared_mut(engine))?
                }
                _ => return Err(BindingError::invalid("invalid rate engine kind")),
            };
            output(out, id)
        })
    }
}
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_black_capfloor_displacement(
    ctx: *mut Context,
    id: u64,
    out: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            output(
                out,
                c.get::<SharedMut<BlackCapFloorEngine>>(id)?
                    .borrow()
                    .displacement(),
            )
        })
    }
}
