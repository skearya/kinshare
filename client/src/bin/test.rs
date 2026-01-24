#![feature(portable_simd)]

use core::{array, simd};
use std::simd::cmp::SimdPartialEq;

fn main() {
    let data = simd::u8x16::from_array(array::from_fn(|i| i as u8 % 2));
    let data2 = simd::u8x16::splat(1);
    let data3 = data.simd_eq(data2);

    dbg!(data3);
}
