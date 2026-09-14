//! Immutable copies of real-valued instrument results.
use crate::boundary::*;
use libitofin::instrument::InstrumentBase;
use libitofin::time::date::Date;
use libitofin::types::Real;
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(crate) struct ResultsSnapshot {
    pub npv: Option<Real>,
    pub error_estimate: Option<Real>,
    pub valuation_date: Option<Date>,
    pub additional_results: BTreeMap<String, Real>,
}
pub(crate) fn snapshot(base: &InstrumentBase) -> ResultsSnapshot {
    let r = base.results();
    ResultsSnapshot {
        npv: r.value,
        error_estimate: r.error_estimate,
        valuation_date: r.valuation_date,
        additional_results: r
            .additional_results
            .iter()
            .filter_map(|(key, value)| {
                value
                    .as_ref()
                    .downcast_ref::<Real>()
                    .map(|v| (key.clone(), *v))
            })
            .collect(),
    }
}
#[repr(C)]
pub struct ItofinResultsFields {
    pub npv: Real,
    pub error_estimate: Real,
    pub valuation_date: i32,
    pub has_npv: u8,
    pub has_error_estimate: u8,
    pub has_valuation_date: u8,
    pub additional_count: usize,
}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_results_fields(
    ctx: *mut Context,
    id: u64,
    out: *mut ItofinResultsFields,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            let r = c.get::<ResultsSnapshot>(id)?;
            output(
                out,
                ItofinResultsFields {
                    npv: r.npv.unwrap_or(0.),
                    error_estimate: r.error_estimate.unwrap_or(0.),
                    valuation_date: r.valuation_date.map(Date::serial_number).unwrap_or(0),
                    has_npv: u8::from(r.npv.is_some()),
                    has_error_estimate: u8::from(r.error_estimate.is_some()),
                    has_valuation_date: u8::from(r.valuation_date.is_some()),
                    additional_count: r.additional_results.len(),
                },
            )
        })
    }
}
/// Read a sorted additional-results entry; key is UTF-8 without a trailing NUL.
/// Capacity zero queries the required size and still returns the real value.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn itofin_results_additional(
    ctx: *mut Context,
    id: u64,
    index: usize,
    key: *mut u8,
    capacity: usize,
    required: *mut usize,
    value: *mut Real,
    error: *mut ItofinError,
) -> i32 {
    unsafe {
        with_context(ctx, error, |c| {
            check_ptr(required)?;
            check_ptr(value)?;
            let r = c.get::<ResultsSnapshot>(id)?;
            let (name, number) = r
                .additional_results
                .iter()
                .nth(index)
                .ok_or_else(|| BindingError::invalid("additional result index out of range"))?;
            output(required, name.len())?;
            output(value, *number)?;
            if capacity == 0 {
                return Ok(());
            }
            if capacity < name.len() {
                return Err(BindingError::invalid("result key buffer too small"));
            }
            check_ptr(key)?;
            std::ptr::copy_nonoverlapping(name.as_ptr(), key, name.len());
            Ok(())
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn results_preserve_missing_values_and_utf8_keys() {
        let mut c = Context::new();
        let id = c
            .insert(ResultsSnapshot {
                npv: Some(0.),
                additional_results: BTreeMap::from([("Δ".into(), 0.5)]),
                ..Default::default()
            })
            .unwrap();
        let mut fields = std::mem::MaybeUninit::uninit();
        let mut size = 0;
        let mut value = 0.;
        unsafe {
            assert_eq!(
                itofin_results_fields(&mut c, id, fields.as_mut_ptr(), std::ptr::null_mut()),
                0
            );
            let fields = fields.assume_init();
            assert_eq!(fields.has_npv, 1);
            assert_eq!(fields.has_error_estimate, 0);
            assert_eq!(fields.has_valuation_date, 0);
            assert_eq!(
                itofin_results_additional(
                    &mut c,
                    id,
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut size,
                    &mut value,
                    std::ptr::null_mut()
                ),
                0
            );
            assert_eq!(size, 2);
            assert_eq!(value, 0.5);
            let mut key = [0; 2];
            assert_eq!(
                itofin_results_additional(
                    &mut c,
                    id,
                    0,
                    key.as_mut_ptr(),
                    1,
                    &mut size,
                    &mut value,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
            assert_eq!(
                itofin_results_additional(
                    &mut c,
                    id,
                    0,
                    key.as_mut_ptr(),
                    2,
                    &mut size,
                    &mut value,
                    std::ptr::null_mut()
                ),
                0
            );
            assert_eq!(&key, "Δ".as_bytes());
            assert_eq!(
                itofin_results_additional(
                    &mut c,
                    id,
                    1,
                    key.as_mut_ptr(),
                    2,
                    &mut size,
                    &mut value,
                    std::ptr::null_mut()
                ),
                INVALID_ARGUMENT
            );
        }
    }
}
