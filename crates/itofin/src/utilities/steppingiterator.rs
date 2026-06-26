//! Iterator advancing in constant steps.
//!
//! Port of `ql/utilities/steppingiterator.hpp`. QuantLib's `step_iterator`
//! adapts a random-access iterator so that each increment advances `step`
//! positions. Over a Rust slice this is naturally an iterator yielding every
//! `step`-th element; we expose it as [`StepIterator`] (plus the [`step_iter`]
//! helper) rather than reimplementing C++ iterator arithmetic.

use crate::types::Size;

/// Iterator over a slice that advances by a fixed step.
pub struct StepIterator<'a, T> {
    data: &'a [T],
    pos: Size,
    step: Size,
}

impl<'a, T> Iterator for StepIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.data.len() {
            return None;
        }
        let item = &self.data[self.pos];
        self.pos += self.step;
        Some(item)
    }
}

/// Creates a stepping iterator over `data` advancing `step` positions each time.
///
/// `step` must be positive, mirroring the C++ pre-condition.
pub fn step_iter<T>(data: &[T], step: Size) -> StepIterator<'_, T> {
    assert!(step > 0, "step must be positive");
    StepIterator { data, pos: 0, step }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advances_in_constant_steps() {
        let data = [0, 1, 2, 3, 4, 5, 6];
        let collected: Vec<i32> = step_iter(&data, 2).copied().collect();
        assert_eq!(collected, vec![0, 2, 4, 6]);
    }

    #[test]
    fn step_of_one_yields_every_element() {
        let data = [10, 20, 30];
        let collected: Vec<i32> = step_iter(&data, 1).copied().collect();
        assert_eq!(collected, vec![10, 20, 30]);
    }

    #[test]
    fn step_larger_than_len_yields_first_only() {
        let data = [10, 20, 30];
        let collected: Vec<i32> = step_iter(&data, 10).copied().collect();
        assert_eq!(collected, vec![10]);
    }

    #[test]
    #[should_panic(expected = "step must be positive")]
    fn zero_step_panics() {
        let _ = step_iter(&[1, 2, 3], 0);
    }
}
