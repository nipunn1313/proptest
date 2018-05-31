//-
// Copyright 2017 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies for working with bit sets.
//!
//! Besides `BitSet` itself, this also defines strategies for all the primitive
//! integer types. These strategies are appropriate for integers which are used
//! as bit flags, etc; e.g., where the most reasonable simplification of `64`
//! is `0` (clearing one bit) and not `63` (clearing one bit but setting 6
//! others). For integers treated as numeric values, see the corresponding
//! modules of the `num` module instead.

use core::marker::PhantomData;
use core::mem;
use core::ops::Range;
use std_facade::fmt;

use bit_set::BitSet;
use rand::{self, Rng};

use strategy::*;
use test_runner::*;

/// Trait for types which can be handled with `BitSetStrategy`.
#[cfg_attr(feature="cargo-clippy", allow(len_without_is_empty))]
pub trait BitSetLike : Clone + fmt::Debug {
    /// Create a new value of `Self` with space for up to `max` bits, all
    /// initialised to zero.
    fn new_bitset(max: usize) -> Self;
    /// Return an upper bound on the greatest bit set _plus one_.
    fn len(&self) -> usize;
    /// Test whether the given bit is set.
    fn test(&self, ix: usize) -> bool;
    /// Set the given bit.
    fn set(&mut self, ix: usize);
    /// Clear the given bit.
    fn clear(&mut self, ix: usize);
    /// Return the number of bits set.
    ///
    /// This has a default for backwards compatibility, which simply does a
    /// linear scan through the bits. Implementations are strongly encouraged
    /// to override this.
    fn count(&self) -> usize {
        let mut n = 0;
        for i in 0..self.len() {
            if self.test(i) {
                n += 1;
            }
        }
        n
    }
}

macro_rules! int_bitset {
    ($typ:ty) => {
        impl BitSetLike for $typ {
            fn new_bitset(_: usize) -> Self { 0 }
            fn len(&self) -> usize { mem::size_of::<$typ>()*8 }
            fn test(&self, ix: usize) -> bool {
                0 != (*self & ((1 as $typ) << ix))
            }
            fn set(&mut self, ix: usize) {
                *self |= (1 as $typ) << ix;
            }
            fn clear(&mut self, ix: usize) {
                *self &= !((1 as $typ) << ix);
            }
            fn count(&self) -> usize {
                self.count_ones() as usize
            }
        }
    }
}
int_bitset!(u8);
int_bitset!(u16);
int_bitset!(u32);
int_bitset!(u64);
int_bitset!(usize);
int_bitset!(i8);
int_bitset!(i16);
int_bitset!(i32);
int_bitset!(i64);
int_bitset!(isize);

impl BitSetLike for BitSet {
    fn new_bitset(max: usize) -> Self {
        BitSet::with_capacity(max)
    }

    fn len(&self) -> usize {
        self.capacity()
    }

    fn test(&self, bit: usize) -> bool {
        self.contains(bit)
    }

    fn set(&mut self, bit: usize) {
        self.insert(bit);
    }

    fn clear(&mut self, bit: usize) {
        self.remove(bit);
    }

    fn count(&self) -> usize {
        self.len()
    }
}

/// Generates values as a set of bits between the two bounds.
///
/// Values are generated by uniformly setting individual bits to 0
/// or 1 between the bounds. Shrinking iteratively clears bits.
#[derive(Clone, Copy, Debug)]
pub struct BitSetStrategy<T : BitSetLike> {
    min: usize,
    max: usize,
    mask: Option<T>
}

impl<T : BitSetLike> BitSetStrategy<T> {
    /// Create a strategy which generates values where bits between `min`
    /// (inclusive) and `max` (exclusive) may be set.
    ///
    /// Due to the generics, the functions in the typed submodules are usually
    /// preferable to calling this directly.
    pub fn new(min: usize, max: usize) -> Self {
        BitSetStrategy {
            min, max, mask: None,
        }
    }

    /// Create a strategy which generates values where any bits set (and only
    /// those bits) in `mask` may be set.
    pub fn masked(mask: T) -> Self {
        BitSetStrategy {
            min: 0,
            max: mask.len(),
            mask: Some(mask)
        }
    }
}

impl<T : BitSetLike> Strategy for BitSetStrategy<T> {
    type Value = BitSetValueTree<T>;

    fn new_value(&self, runner: &mut TestRunner) -> NewTree<Self> {
        let mut inner = T::new_bitset(self.max);
        for bit in self.min..self.max {
            if self.mask.as_ref().map_or(true, |mask| mask.test(bit)) &&
                runner.rng().gen()
            {
                inner.set(bit);
            }
        }

        Ok(BitSetValueTree {
            inner,
            shrink: self.min,
            prev_shrink: None,
            min_count: 0
        })
    }
}

/// Generates bit sets with a particular number of bits set.
///
/// Specifically, this strategy is given both a size range and a bit range. To
/// produce a new value, it selects a size, then uniformly selects that many
/// bits from within the bit range.
///
/// Shrinking happens as with [`BitSetStrategy`](struct.BitSetStrategy.html).
#[derive(Clone, Debug)]
pub struct SampledBitSetStrategy<T : BitSetLike> {
    size: Range<usize>,
    bits: Range<usize>,
    _marker: PhantomData<T>,
}

impl<T : BitSetLike> SampledBitSetStrategy<T> {
    /// Create a strategy which generates values where bits within the bounds
    /// given by `bits` may be set. The number of bits that are set is chosen
    /// to be in the range given by `size`.
    ///
    /// Due to the generics, the functions in the typed submodules are usually
    /// preferable to calling this directly.
    ///
    /// ## Panics
    ///
    /// Panics if `size` includes a value that is greater than the number of
    /// bits in `bits`.
    pub fn new(size: Range<usize>, bits: Range<usize>) -> Self {
        let available_bits = bits.end - bits.start;
        assert!(size.end <= available_bits + 1,
                "Illegal SampledBitSetStrategy: have {} bits available, \
                 but requested size is {}..{}",
                available_bits, size.start, size.end);
        SampledBitSetStrategy {
            size, bits, _marker: PhantomData
        }
    }
}

impl<T : BitSetLike> Strategy for SampledBitSetStrategy<T> {
    type Value = BitSetValueTree<T>;

    fn new_value(&self, runner: &mut TestRunner) -> NewTree<Self> {
        let mut bits = T::new_bitset(self.bits.end);
        let count = runner.rng().gen_range(self.size.start, self.size.end);
        for bit in
            rand::seq::sample_iter(runner.rng(), self.bits.clone(), count)
            .expect("not enough bits to sample")
        {
            bits.set(bit);
        }

        Ok(BitSetValueTree {
            inner: bits,
            shrink: self.bits.start,
            prev_shrink: None,
            min_count: self.size.start,
        })
    }
}

/// Value tree produced by `BitSetStrategy` and `SampledBitSetStrategy`.
#[derive(Clone, Copy, Debug)]
pub struct BitSetValueTree<T : BitSetLike> {
    inner: T,
    shrink: usize,
    prev_shrink: Option<usize>,
    min_count: usize,
}

impl<T : BitSetLike> ValueTree for BitSetValueTree<T> {
    type Value = T;

    fn current(&self) -> T {
        self.inner.clone()
    }

    fn simplify(&mut self) -> bool {
        if self.inner.count() <= self.min_count {
            return false;
        }

        while self.shrink < self.inner.len() &&
            !self.inner.test(self.shrink)
        { self.shrink += 1; }

        if self.shrink >= self.inner.len() {
            self.prev_shrink = None;
            false
        } else {
            self.prev_shrink = Some(self.shrink);
            self.inner.clear(self.shrink);
            self.shrink += 1;
            true
        }
    }

    fn complicate(&mut self) -> bool {
        if let Some(bit) = self.prev_shrink.take() {
            self.inner.set(bit);
            true
        } else {
            false
        }
    }
}

macro_rules! int_api {
    ($typ:ident, $max:expr) => {
        #[allow(missing_docs)]
        pub mod $typ {
            use super::*;

            /// Generates integers where all bits may be set.
            pub const ANY: BitSetStrategy<$typ> = BitSetStrategy {
                min: 0,
                max: $max,
                mask: None,
            };

            /// Generates values where bits between the given bounds may be
            /// set.
            pub fn between(min: usize, max: usize) -> BitSetStrategy<$typ> {
                BitSetStrategy::new(min, max)
            }

            /// Generates values where any bits set in `mask` (and no others)
            /// may be set.
            pub fn masked(mask: $typ) -> BitSetStrategy<$typ> {
                BitSetStrategy::masked(mask)
            }

            /// Create a strategy which generates values where bits within the
            /// bounds given by `bits` may be set. The number of bits that are
            /// set is chosen to be in the range given by `size`.
            ///
            /// ## Panics
            ///
            /// Panics if `size` includes a value that is greater than the
            /// number of bits in `bits`.
            pub fn sampled(size: Range<usize>, bits: Range<usize>)
                           -> SampledBitSetStrategy<$typ> {
                SampledBitSetStrategy::new(size, bits)
            }
        }
    }
}

int_api!(u8, 8);
int_api!(u16, 16);
int_api!(u32, 32);
int_api!(u64, 64);
int_api!(i8, 8);
int_api!(i16, 16);
int_api!(i32, 32);
int_api!(i64, 64);

macro_rules! minimal_api {
    ($md:ident, $typ:ty) => {
        #[allow(missing_docs)]
        pub mod $md {
            use super::*;

            /// Generates values where bits between the given bounds may be
            /// set.
            pub fn between(min: usize, max: usize) -> BitSetStrategy<$typ> {
                BitSetStrategy::new(min, max)
            }

            /// Generates values where any bits set in `mask` (and no others)
            /// may be set.
            pub fn masked(mask: $typ) -> BitSetStrategy<$typ> {
                BitSetStrategy::masked(mask)
            }

            /// Create a strategy which generates values where bits within the
            /// bounds given by `bits` may be set. The number of bits that are
            /// set is chosen to be in the range given by `size`.
            ///
            /// ## Panics
            ///
            /// Panics if `size` includes a value that is greater than the
            /// number of bits in `bits`.
            pub fn sampled(size: Range<usize>, bits: Range<usize>)
                           -> SampledBitSetStrategy<$typ> {
                SampledBitSetStrategy::new(size, bits)
            }
        }
    }
}
minimal_api!(usize, usize);
minimal_api!(isize, isize);
minimal_api!(bitset, BitSet);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn generates_values_in_range() {
        let input = u32::between(4, 8);

        let mut runner = TestRunner::default();
        for _ in 0..256 {
            let value = input.new_value(&mut runner).unwrap().current();
            assert!(0 == value & !0xF0u32,
                    "Generate value {}", value);
        }
    }

    #[test]
    fn generates_values_in_mask() {
        let mut accum = 0;

        let mut runner = TestRunner::default();
        let input = u32::masked(0xdeadbeef);
        for _ in 0..1024 {
            accum |= input.new_value(&mut runner).unwrap().current();
        }

        assert_eq!(0xdeadbeef, accum);
    }

    #[test]
    fn mask_bounds_for_bitset_correct() {
        let mut seen_0 = false;
        let mut seen_2 = false;

        let mut mask = BitSet::new();
        mask.insert(0);
        mask.insert(2);

        let mut runner = TestRunner::default();
        let input = bitset::masked(mask);
        for _ in 0..32 {
            let v = input.new_value(&mut runner).unwrap().current();
            seen_0 |= v.contains(0);
            seen_2 |= v.contains(2);
        }

        assert!(seen_0);
        assert!(seen_2);
    }

    #[test]
    fn shrinks_to_zero() {
        let input = u32::between(4, 24);

        let mut runner = TestRunner::default();
        for _ in 0..256 {
            let mut value = input.new_value(&mut runner).unwrap();
            let mut prev = value.current();
            while value.simplify() {
                let v = value.current();
                assert!(1 == (prev & !v).count_ones(),
                        "Shrank from {} to {}", prev, v);
                prev = v;
            }

            assert_eq!(0, value.current());
        }
    }

    #[test]
    fn complicates_to_previous() {
        let input = u32::between(4, 24);

        let mut runner = TestRunner::default();
        for _ in 0..256 {
            let mut value = input.new_value(&mut runner).unwrap();
            let orig = value.current();
            if value.simplify() {
                assert!(value.complicate());
                assert_eq!(orig, value.current());
            }
        }
    }

    #[test]
    fn sampled_selects_correct_sizes_and_bits() {
        let input = u32::sampled(4..8, 10..20);
        let mut seen_counts = [0; 32];
        let mut seen_bits = [0; 32];

        let mut runner = TestRunner::default();
        for _ in 0..2048 {
            let value = input.new_value(&mut runner).unwrap().current();
            let count = value.count_ones() as usize;
            assert!(count >= 4 && count < 8);
            seen_counts[count] += 1;

            for bit in 0..32 {
                if 0 != value & (1 << bit) {
                    assert!(bit >= 10 && bit < 20);
                    seen_bits[bit] += value;
                }
            }
        }

        for i in 4..8 {
            assert!(seen_counts[i] >= 256 && seen_counts[i] < 1024);
        }

        let least_seen_bit_count =
            seen_bits[10..20].iter().cloned().min().unwrap();
        let most_seen_bit_count =
            seen_bits[10..20].iter().cloned().max().unwrap();
        assert_eq!(1, most_seen_bit_count / least_seen_bit_count);
    }

    #[test]
    fn sampled_doesnt_shrink_below_min_size() {
        let input = u32::sampled(4..8, 10..20);

        let mut runner = TestRunner::default();
        for _ in 0..256 {
            let mut value = input.new_value(&mut runner).unwrap();
            while value.simplify() { }

            assert_eq!(4, value.current().count_ones());
        }
    }

    #[test]
    fn test_sanity() {
        check_strategy_sanity(u32::masked(0xdeadbeef), None);
    }
}
