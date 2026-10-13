use crate::boundary::{BindingError, BindingResult};
use libitofin::instrument::Instrument;
use libitofin::instruments::{Bond, CallableFixedRateBond, CallableZeroCouponBond};
use libitofin::shared::SharedMut;
use libitofin::time::date::Date;
use std::cell::Ref;

#[derive(Clone)]
pub(crate) enum BondBacking {
    Plain(SharedMut<Bond>),
    CallableFixed(SharedMut<CallableFixedRateBond>),
    CallableZero(SharedMut<CallableZeroCouponBond>),
}
impl BondBacking {
    pub(crate) fn borrow(&self) -> Ref<'_, Bond> {
        match self {
            Self::Plain(bond) => bond.borrow(),
            Self::CallableFixed(bond) => Ref::map(bond.borrow(), |bond| bond.bond()),
            Self::CallableZero(bond) => Ref::map(bond.borrow(), |bond| bond.bond()),
        }
    }
    pub(crate) fn plain(&self) -> BindingResult<&SharedMut<Bond>> {
        match self {
            Self::Plain(bond) => Ok(bond),
            Self::CallableFixed(_) | Self::CallableZero(_) => Err(BindingError::invalid(
                "discounting bond engine does not support callable bonds",
            )),
        }
    }
    pub(crate) fn with_instrument<R>(&self, f: impl FnOnce(&dyn Instrument) -> R) -> R {
        match self {
            Self::Plain(bond) => f(&*bond.borrow()),
            Self::CallableFixed(bond) => f(&*bond.borrow()),
            Self::CallableZero(bond) => f(&*bond.borrow()),
        }
    }
    pub(crate) fn with_instrument_mut<R>(&self, f: impl FnOnce(&mut dyn Instrument) -> R) -> R {
        match self {
            Self::Plain(bond) => f(&mut *bond.borrow_mut()),
            Self::CallableFixed(bond) => f(&mut *bond.borrow_mut()),
            Self::CallableZero(bond) => f(&mut *bond.borrow_mut()),
        }
    }
    pub(crate) fn real(&self, field: i32, settlement: Date) -> BindingResult<f64> {
        if field >= 4 {
            let bond = self.borrow();
            return match field {
                4 => Ok(bond.accrued_amount(Some(settlement))?),
                5 => Ok(bond.notional(Some(settlement))?),
                _ => Err(BindingError::invalid("invalid bond real field")),
            };
        }
        match self {
            Self::Plain(bond) => {
                let mut bond = bond.borrow_mut();
                match field {
                    0 => Ok(bond.npv()?),
                    1 => Ok(bond.settlement_value()?),
                    2 | 3 => {
                        let notional = bond.notional(Some(settlement))?;
                        let dirty = if notional == 0.0 {
                            0.0
                        } else {
                            let value = bond.settlement_value()?;
                            let scaled = value * 100.0;
                            if scaled.is_finite() {
                                scaled / notional
                            } else {
                                value / notional * 100.0
                            }
                        };
                        Ok(if field == 2 {
                            dirty - bond.accrued_amount(Some(settlement))?
                        } else {
                            dirty
                        })
                    }
                    _ => Err(BindingError::invalid("invalid bond real field")),
                }
            }
            Self::CallableFixed(bond) => {
                let mut bond = bond.borrow_mut();
                match field {
                    0 => Ok(bond.npv()?),
                    1 => Ok(bond.settlement_value()?),
                    2 => Ok(bond.clean_price()?),
                    3 => Ok(bond.dirty_price()?),
                    _ => Err(BindingError::invalid("invalid bond real field")),
                }
            }
            Self::CallableZero(bond) => {
                let mut bond = bond.borrow_mut();
                match field {
                    0 => Ok(bond.npv()?),
                    1 => Ok(bond.settlement_value()?),
                    2 => Ok(bond.clean_price()?),
                    3 => Ok(bond.dirty_price()?),
                    _ => Err(BindingError::invalid("invalid bond real field")),
                }
            }
        }
    }
}
