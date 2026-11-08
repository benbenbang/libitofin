//! Yield-curve discount jumps on the piecewise and global bootstrap constructors.
//!
//! Jumps on the node curves of `itofin_node_curve_new` are a tracked follow-up.
use crate::bootstrap_api::global_curve;
use crate::bootstrap_callbacks::ItofinBootstrapCallbacks;
use crate::boundary::*;
use crate::curves_api::{curve, piecewise_curve};
use crate::iterative_bootstrap_api::{
    CurveAnchor, ItofinIterativeBootstrapOptions, curve_jumps, moving_anchor,
};
use crate::time_api::date;

/// A zero settings handle selects the fixed reference date; a nonzero one
/// selects a reference date settlement_days business days on calendar past
/// the evaluation date of settings. settings must be the SAME instance the
/// helpers' index carries: a different instance moves the curve's reference
/// date while the helpers stay put.
#[repr(C)]
pub struct ItofinPiecewiseYieldConfig {
    pub reference_date: i32,
    pub settlement_days: u32,
    pub calendar: u64,
    pub day_counter: u64,
    pub settings: u64,
}

fn config_anchor(c: &Context, a: &ItofinPiecewiseYieldConfig) -> BindingResult<CurveAnchor> {
    if a.settings == 0 {
        Ok(CurveAnchor::Fixed(date(a.reference_date)?))
    } else {
        moving_anchor(c, a.settlement_days, a.calendar, a.settings)
    }
}

/// The itofin_piecewise_curve_new twin taking a fixed or moving reference
/// date, optional iterative options (null for the defaults, iterative
/// algorithm only) and multiplicative discount jumps on SimpleQuote handles.
/// Empty dates generate December 31 jump dates from the reference year,
/// which on a moving curve needs the evaluation date already set: an unset
/// one is an error, never a clock fallback.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_piecewise_curve_with_jumps_new(
    ctx: *mut Context,
    config: *const ItofinPiecewiseYieldConfig,
    helpers: *const u64,
    len: usize,
    kind: i32,
    algorithm: i32,
    additional: *const u64,
    additional_len: usize,
    options: *const ItofinIterativeBootstrapOptions,
    jumps: *const u64,
    jump_count: usize,
    dates: *const i32,
    date_count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(config)?;
            check_ptr(out)?;
            let a = &*config;
            let options = if options.is_null() {
                Default::default()
            } else if algorithm != 0 {
                return Err(BindingError::invalid(
                    "iterative options require iterative bootstrap",
                ));
            } else {
                (*options).parse()?
            };
            let anchor = config_anchor(c, a)?;
            let jumps = curve_jumps(c, jumps, jump_count, dates, date_count)?;
            let (h, x) = (
                input_slice(helpers, len)?,
                input_slice(additional, additional_len)?,
            );
            piecewise_curve(
                c,
                anchor,
                h,
                a.day_counter,
                kind,
                algorithm,
                x,
                options,
                jumps,
                out,
            )
        })
    }
}

/// The itofin_global_curve_new twin taking a fixed or moving reference date
/// and multiplicative discount jumps, dated as in
/// itofin_piecewise_curve_with_jumps_new. Callback ownership follows
/// itofin_global_curve_new, including on a jump argument error.
/// # Safety
/// As itofin_global_curve_new.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_global_curve_with_jumps_new(
    ctx: *mut Context,
    config: *const ItofinPiecewiseYieldConfig,
    helpers: *const u64,
    len: usize,
    kind: i32,
    additional: *const u64,
    additional_len: usize,
    variables: u64,
    callbacks: *const ItofinBootstrapCallbacks,
    adopted: *mut bool,
    jumps: *const u64,
    jump_count: usize,
    dates: *const i32,
    date_count: usize,
    out: *mut u64,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        let dc = if check_ptr(config).is_ok() {
            (*config).day_counter
        } else {
            0
        };
        let anchor = move |c: &Context| {
            check_ptr(config)?;
            let anchor = config_anchor(c, &*config)?;
            Ok((
                anchor,
                curve_jumps(c, jumps, jump_count, dates, date_count)?,
            ))
        };
        global_curve(
            ctx,
            anchor,
            helpers,
            len,
            dc,
            kind,
            additional,
            additional_len,
            variables,
            callbacks,
            adopted,
            out,
            error,
        )
    }
}

/// Jump dates in constructor order and their times against the current
/// reference date, for any yield curve (a jump-free curve has none).
/// Capacity zero queries the required length. A null `times` pointer
/// requests dates only, without resolving the reference date.
#[unsafe(no_mangle)]
/// # Safety
/// Pointers must be aligned, live and valid for their stated lengths. Outputs
/// must not overlap inputs or other outputs. Any context and its handles must
/// belong to the calling thread; serialize calls including destruction.
/// See the crate-level C caller contract for lifetime requirements.
pub unsafe extern "C" fn itofin_curve_jumps(
    ctx: *mut Context,
    id: u64,
    dates: *mut i32,
    times: *mut f64,
    capacity: usize,
    count: *mut usize,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(count)?;
            let curve = curve(c, id)?.current_link()?;
            let jump_dates = curve.jump_dates();
            output(count, jump_dates.len())?;
            if capacity == 0 {
                return Ok(());
            }
            check_ptr(dates)?;
            if capacity < jump_dates.len() {
                return Err(BindingError::invalid("jump buffer too small"));
            }
            if !times.is_null() {
                check_ptr(times)?;
                for (i, t) in curve.jump_times()?.into_iter().enumerate() {
                    output(times.add(i), t)?;
                }
            }
            for (i, d) in jump_dates.iter().enumerate() {
                output(dates.add(i), d.serial_number())?;
            }
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curves_api::{itofin_curve_nodes, itofin_piecewise_curve_new};
    use crate::iterative_bootstrap_api::itofin_iterative_bootstrap_options_default;
    use libitofin::handle::Handle;
    use libitofin::indexes::Euribor;
    use libitofin::quotes::SimpleQuote;
    use libitofin::settings::Settings;
    use libitofin::shared::{Shared, shared};
    use libitofin::termstructures::RateHelper;
    use libitofin::termstructures::yields::DepositRateHelper;
    use libitofin::time::date::{Date, Month};
    use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
    use std::mem::MaybeUninit;
    use std::ptr::{null, null_mut};

    unsafe fn nodes(c: &mut Context, id: u64) -> Vec<f64> {
        let (mut ds, mut vs, mut n) = ([0; 4], [0.0; 4], 0);
        let rc = unsafe {
            itofin_curve_nodes(
                c,
                id,
                ds.as_mut_ptr(),
                vs.as_mut_ptr(),
                4,
                &mut n,
                null_mut(),
            )
        };
        assert_eq!(rc, 0);
        vs[..n].to_vec()
    }

    /// A 3M deposit pins the total discount at its pillar, so the jumped node
    /// times the jump equals the plain node on the iterative and the global
    /// algorithm.
    #[test]
    fn jumped_piecewise_nodes_carry_the_jump_on_every_algorithm() {
        let mut c = Context::new();
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(15, Month::June, 2026));
        let index = Euribor::three_months(Handle::empty(), settings);
        let mut helper = || {
            c.insert(DepositRateHelper::from_rate(0.03, &index) as Shared<dyn RateHelper>)
                .unwrap()
        };
        let helpers: Vec<u64> = (0..4).map(|_| helper()).collect();
        let dc = c.insert(Actual365Fixed::new()).unwrap();
        let reference = Date::new(17, Month::June, 2026).serial_number();
        let jump_date = Date::new(1, Month::August, 2026).serial_number();
        let quote = c.insert(shared(SimpleQuote::new(0.99))).unwrap();
        let config = ItofinPiecewiseYieldConfig {
            reference_date: reference,
            settlement_days: 0,
            calendar: 0,
            day_counter: dc,
            settings: 0,
        };
        let (mut plain, mut jumped) = (0, 0);
        unsafe {
            for algorithm in [0, 1] {
                let h = &helpers[2 * algorithm as usize..];
                let rc = itofin_piecewise_curve_new(
                    &mut c,
                    reference,
                    &h[0],
                    1,
                    dc,
                    0,
                    algorithm,
                    null(),
                    0,
                    &mut plain,
                    null_mut(),
                );
                assert_eq!(rc, 0);
                let rc = itofin_piecewise_curve_with_jumps_new(
                    &mut c,
                    &config,
                    &h[1],
                    1,
                    0,
                    algorithm,
                    null(),
                    0,
                    null(),
                    &quote,
                    1,
                    &jump_date,
                    1,
                    &mut jumped,
                    null_mut(),
                );
                assert_eq!(rc, 0);
                let (p, j) = (nodes(&mut c, plain), nodes(&mut c, jumped));
                assert_eq!((p.len(), j.len()), (2, 2));
                assert!((j[1] * 0.99 - p[1]).abs() <= 1e-12, "{j:?} vs {p:?}");
                assert!((j[1] - p[1]).abs() > 1e-4);
            }

            let mut options = MaybeUninit::uninit();
            assert_eq!(
                itofin_iterative_bootstrap_options_default(options.as_mut_ptr(), null_mut()),
                0
            );
            let options = options.assume_init();
            for (cfg, algorithm) in [(null(), 0), (&config as *const _, 1)] {
                let rc = itofin_piecewise_curve_with_jumps_new(
                    &mut c,
                    cfg,
                    &helpers[0],
                    1,
                    0,
                    algorithm,
                    null(),
                    0,
                    &options,
                    &quote,
                    1,
                    &jump_date,
                    1,
                    &mut jumped,
                    null_mut(),
                );
                assert_eq!(rc, INVALID_ARGUMENT);
            }
        }
    }
}
