//! Shared test utilities.
//!
//! Port of the `Flag` utility from QuantLib's `test-suite/utilities.hpp`: an
//! observer recording whether it was raised. Pre-existing per-module copies
//! (`settings`, `handle`, `patterns::lazyobject`) predate this module and can
//! migrate here over time.

use crate::patterns::observable::Observer;
use crate::shared::{SharedMut, shared_mut};

/// Observer recording whether it was notified.
pub(crate) struct Flag {
    up: bool,
}

impl Flag {
    pub(crate) fn new() -> SharedMut<Flag> {
        shared_mut(Flag { up: false })
    }

    pub(crate) fn lower(flag: &SharedMut<Flag>) {
        flag.borrow_mut().up = false;
    }

    pub(crate) fn is_up(flag: &SharedMut<Flag>) -> bool {
        flag.borrow().up
    }
}

impl Observer for Flag {
    fn update(&mut self) {
        self.up = true;
    }
}

/// Upcasts a flag to the `dyn Observer` handle expected by registration.
pub(crate) fn as_observer(flag: &SharedMut<Flag>) -> SharedMut<dyn Observer> {
    flag.clone()
}
