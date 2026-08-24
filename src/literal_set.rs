// Copyright 2024 tison <wander4096@gmail.com>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::fmt;
use std::ops::RangeInclusive;

/// An allocation-free set of literals in the range `0..64`.
///
/// Insertion is idempotent, membership checks take constant time, and iteration yields values in
/// ascending order. Cron fields use at most `0..=59`, so every literal fits in one `u64`.
#[derive(Clone, Copy, Default)]
pub struct LiteralSet(u64);

impl LiteralSet {
    pub fn insert(&mut self, value: u8) {
        debug_assert!(value < u64::BITS as u8);
        self.0 |= 1_u64 << value;
    }

    pub fn insert_range(&mut self, range: RangeInclusive<u8>) {
        let (start, end) = range.into_inner();
        debug_assert!(start <= end);
        debug_assert!(end < u64::BITS as u8);

        let from_start = u64::MAX << start;
        let through_end = u64::MAX >> (u64::BITS - 1 - u32::from(end));
        self.0 |= from_start & through_end;
    }

    pub fn insert_step(&mut self, range: RangeInclusive<u8>, step: u8) {
        let (mut value, end) = range.into_inner();
        debug_assert!(value <= end);
        debug_assert!(end < u64::BITS as u8);
        debug_assert!(step > 0);

        loop {
            self.0 |= 1_u64 << value;
            let next = value + step;
            if next > end {
                break;
            }
            value = next;
        }
    }

    pub fn contains(&self, value: u8) -> bool {
        debug_assert!(value < u64::BITS as u8);
        self.0 & (1_u64 << value) != 0
    }

    pub fn iter(&self) -> impl Iterator<Item = u8> + '_ {
        (0..u64::BITS as u8).filter(|value| self.contains(*value))
    }
}

impl fmt::Debug for LiteralSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::LiteralSet;

    #[test]
    fn preserves_set_contract_at_both_boundaries() {
        let mut set = LiteralSet::default();
        set.insert(63);
        set.insert(0);
        set.insert(63);
        set.insert_range(2..=4);
        set.insert_step(5..=9, 2);

        assert!(set.contains(0));
        assert!(set.contains(63));
        assert!(!set.contains(1));
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            vec![0, 2, 3, 4, 5, 7, 9, 63]
        );
        assert_eq!(format!("{set:?}"), "{0, 2, 3, 4, 5, 7, 9, 63}");

        set.insert_range(0..=63);
        assert_eq!(set.iter().count(), 64);
    }
}
