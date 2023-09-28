use crate::math::int_mod::IntMod;
use crate::math::utils::{floor_log, mod_inverse, reverse_bits};
use std::arch::x86_64::{__m256, __m256i};
use std::num::Wrapping;

/// Compile time lookup table for NTT-related operations
struct NTTTable<const D: usize, const N: u64, const W: u64> {}

#[repr(C, align(64))]
#[derive(Clone)]
pub struct Aligned64<T>(pub T);

#[derive(Copy, Clone)]
struct MulTable<const N: u64> {
    value: IntMod<N>,
    ratio32: u64,
}

impl<const N: u64> MulTable<N> {
    const fn zero() -> Self {
        MulTable {
            value: IntMod::from_u64_const(0_u64),
            ratio32: 0,
        }
    }
}

impl<const D: usize, const N: u64, const W: u64> NTTTable<D, N, W> {
    const W_POWERS_BIT_REVERSED: [MulTable<N>; D] = get_powers_bit_reversed::<D, N, W>(false);
    const W_INV_POWERS_BIT_REVERSED: [MulTable<N>; D] = get_powers_bit_reversed::<D, N, W>(true);
    const LOG_D: usize = floor_log(2, D as u64);
    const INV_D: IntMod<N> = IntMod::from_u64_const(mod_inverse(D as u64, N));
    const INV_D_RATIO32: u64 = get_ratio32::<N>(mod_inverse(D as u64, N) % N);
}

const fn get_ratio32<const N: u64>(a: u64) -> u64 {
    (a << 32) / N
}

const fn get_powers_bit_reversed<const D: usize, const N: u64, const W: u64>(
    invert: bool,
) -> [MulTable<N>; D] {
    let root = if invert {
        IntMod::from_u64_const(mod_inverse(W, N))
    } else {
        IntMod::from_u64_const(W)
    };

    let mut table = [MulTable::zero(); D];
    let mut cur = IntMod::from_u64_const(1_u64);
    let mut idx = 0;

    while idx < D {
        let ratio32 = get_ratio32::<N>(cur.into_u64_const());
        table[reverse_bits::<D>(idx)] = MulTable {
            value: cur,
            ratio32,
        };

        cur = IntMod::mul_const(cur, root);
        idx += 1
    }
    table
}

///
/// Compute a representative of `lhs * rhs mod N` in the range `[0, 2N)` on all four lanes.
/// - The input `lhs` must be in the range `[0, 4N)`.
/// - The modulus `N` must satisfy `N < 2^30`.
/// - `rhs` resp. `rhs_ratio32` must be in the range `[0, N)`. The latter value is to be computed via
/// `get_ratio32::<N>` of the former value.
/// - `neg_modulus` must have `-N` in all lanes, e.g. via `_mm256_set1_epi64x(-(N as i64))`
///
unsafe fn _mm256_mod_mul32(
    lhs: __m256i,
    rhs: __m256i,
    rhs_ratio32: __m256i,
    neg_modulus: __m256i,
) -> __m256i {
    use std::arch::x86_64::*;
    let quotient = _mm256_srli_epi64::<32>(_mm256_mul_epu32(rhs_ratio32, lhs));
    let lhs_times_rhs = _mm256_mullo_epi32(lhs, rhs);
    let neg_modulus_times_quotient = _mm256_mullo_epi32(neg_modulus, quotient);
    _mm256_add_epi32(lhs_times_rhs, neg_modulus_times_quotient)
}

///
/// Reduce the input from the range `[0, 2*modulus)` to `[0, modulus)` on all four lanes.
/// - The modulus must be `< 2^31`.
///
unsafe fn _mm256_reduce_half(value: __m256i, modulus: __m256i) -> __m256i {
    use std::arch::x86_64::*;
    _mm256_min_epu32(value, _mm256_sub_epi32(value, modulus))
}

// #[cfg(not(target_feature = "avx2"))]
// pub fn ntt_neg_forward<const D: usize, const N: u64, const W: u64>(
//     values: &mut Aligned64<[IntMod<N>; D]>,
// ) {
//     let _ = values;
//     todo!();
// }

// #[cfg(target_feature = "avx2")]
pub fn ntt_neg_forward<const D: usize, const N: u64, const W: u64>(
    values: &mut Aligned64<[IntMod<N>; D]>,
) {
    use std::arch::x86_64::*;

    let values = values as *mut Aligned64<[IntMod<N>; D]>;
    let values = values as *mut Aligned64<[u64; D]>;
    let values = unsafe { &mut *values };

    let modulus = unsafe { _mm256_set1_epi64x(N as i64) };
    let double_modulus = unsafe { _mm256_set1_epi64x(2 * N as i64) };
    let neg_modulus = unsafe { _mm256_set1_epi64x(-(N as i64)) };

    // Algorithm 2 of https://arxiv.org/pdf/2103.16400.pdf
    for round in 0..NTTTable::<D, N, W>::LOG_D {
        let block_count = 1_usize << round;
        let block_half_stride = D >> (1_usize + round);
        let block_stride = 2 * block_half_stride;
        for block_idx in 0..block_count {
            let block_left_half_range =
                (block_idx * block_stride)..(block_idx * block_stride + block_half_stride);

            let w_table = unsafe {
                *NTTTable::<D, N, W>::W_POWERS_BIT_REVERSED.get_unchecked(block_count + block_idx)
            };

            if block_left_half_range.len() < 4 {
                for left_idx in block_left_half_range {
                    let right_idx = left_idx + block_half_stride;
                    unsafe {
                        let x = *values.0.get_unchecked(left_idx);
                        let y = *values.0.get_unchecked(right_idx);
                        let w = w_table.value.into_u64_const();
                        let ratio = w_table.ratio32;

                        // Butterfly
                        let x = if x >= 2 * N { x - 2 * N } else { x };
                        let quotient = (ratio * y) >> 32;
                        let product = (Wrapping(w as u32) * Wrapping(y as u32)
                            - Wrapping(N as u32) * Wrapping(quotient as u32))
                        .0;
                        let x_new = x + product as u64;
                        let y_new = x + (2 * N - product as u64);

                        *values.0.get_unchecked_mut(left_idx) = x_new;
                        *values.0.get_unchecked_mut(right_idx) = y_new;
                    }
                }
            } else {
                unsafe {
                    let w = _mm256_set1_epi64x(w_table.value.into_u64_const() as i64);
                    let w_ratio32 = _mm256_set1_epi64x(w_table.ratio32 as i64);

                    for left_idx in block_left_half_range.step_by(4) {
                        let right_idx = left_idx + block_half_stride;
                        let left_ptr =
                            values.0.get_unchecked(left_idx) as *const u64 as *const __m256i;
                        let right_ptr =
                            values.0.get_unchecked(right_idx) as *const u64 as *const __m256i;

                        // Butterfly
                        let x = _mm256_load_si256(left_ptr);
                        let y = _mm256_load_si256(right_ptr);
                        let x = _mm256_reduce_half(x, double_modulus);
                        let product = _mm256_mod_mul32(y, w, w_ratio32, neg_modulus);
                        let x_new_vec = _mm256_add_epi64(x, product);
                        let y_new_vec =
                            _mm256_add_epi64(x, _mm256_sub_epi64(double_modulus, product));

                        _mm256_store_si256(left_ptr as *mut __m256i, x_new_vec);
                        _mm256_store_si256(right_ptr as *mut __m256i, y_new_vec);
                    }
                }
            }
        }
    }

    unsafe {
        for i in (0..D).step_by(4) {
            let val = _mm256_load_si256(values.0.get_unchecked(i) as *const u64 as *const __m256i);
            let val = _mm256_reduce_half(val, double_modulus);
            let val = _mm256_reduce_half(val, modulus);
            _mm256_store_si256(
                values.0.get_unchecked_mut(i) as *mut u64 as *mut __m256i,
                val,
            );
        }
    }
}

// #[cfg(not(target_feature = "avx2"))]
// pub fn ntt_neg_backward<const D: usize, const N: u64, const W: u64>(
//     values: &mut Aligned64<[IntMod<N>; D]>,
// ) {
//     let _ = values;
//     todo!();
// }

// #[cfg(target_feature = "avx2")]
pub fn ntt_neg_backward<const D: usize, const N: u64, const W: u64>(
    values: &mut Aligned64<[IntMod<N>; D]>,
) {
    use std::arch::x86_64::*;

    let values = values as *mut Aligned64<[IntMod<N>; D]>;
    let values = values as *mut Aligned64<[u64; D]>;
    let values = unsafe { &mut *values };

    let modulus = unsafe { _mm256_set1_epi64x(N as i64) };
    let double_modulus = unsafe { _mm256_set1_epi64x(2 * N as i64) };
    let neg_modulus = unsafe { _mm256_set1_epi64x(-(N as i64)) };

    // Algorithm 3 of https://arxiv.org/pdf/2103.16400.pdf
    for round in 0..NTTTable::<D, N, W>::LOG_D {
        let block_count = D >> (1_usize + round);
        let block_half_stride = 1 << round;
        let block_stride = 2 * block_half_stride;

        for block_idx in 0..block_count {
            let block_left_half_range =
                (block_idx * block_stride)..(block_idx * block_stride + block_half_stride);

            let w_table = unsafe {
                NTTTable::<D, N, W>::W_INV_POWERS_BIT_REVERSED
                    .get_unchecked(block_count + block_idx)
            };

            // if block_left_half_range.len() < 4 {
            for left_idx in block_left_half_range {
                let right_idx = left_idx + block_half_stride;
                unsafe {
                    // Butterfly
                    let x = *values.0.get_unchecked(left_idx);
                    let y = *values.0.get_unchecked(right_idx);

                    let x_new = x + y;
                    let x_new = if x_new >= 2 * N { x_new - 2 * N } else { x_new };
                    let sum = x - y + 2 * N;
                    let quotient = (w_table.ratio32 * sum) >> 32;
                    let y_new = (Wrapping(u64::from(w_table.value) as u32) * Wrapping(sum as u32)
                        - Wrapping(N as u32) * Wrapping(quotient as u32))
                    .0 as u64;

                    *values.0.get_unchecked_mut(left_idx) = x_new;
                    *values.0.get_unchecked_mut(right_idx) = y_new;
                }
            }
            // }
            // else {
            //     unsafe {
            //         let w = _mm256_set1_epi64x(w_table.value.into_u64_const() as i64);
            //         let ratio = _mm256_set1_epi64x(w_table.ratio32 as i64);
            //     }
            // }
        }
    }

    unsafe {
        let inv_d = _mm256_set1_epi64x(u64::from(NTTTable::<D, N, W>::INV_D) as i64);
        let inv_d_ratio32 = _mm256_set1_epi64x(NTTTable::<D, N, W>::INV_D_RATIO32 as i64);

        for i in (0..D).step_by(4) {
            let val = _mm256_load_si256(values.0.get_unchecked(i) as *const u64 as *const __m256i);
            let val = _mm256_mod_mul32(val, inv_d, inv_d_ratio32, neg_modulus);
            let val = _mm256_reduce_half(val, modulus);
            _mm256_store_si256(
                values.0.get_unchecked_mut(i) as *mut u64 as *mut __m256i,
                val,
            );
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::math::int_mod_cyclo::IntModCyclo;
    use crate::math::int_mod_cyclo_eval::IntModCycloEval;
    use crate::math::int_mod_poly::IntModPoly;
    use crate::math::number_theory::find_sqrt_primitive_root;
    use crate::math::rand_sampled::RandUniformSampled;
    use crate::math::ring_elem::RingElement;
    use rand::SeedableRng;
    use rand_chacha::ChaCha20Rng;
    use std::iter;
    use std::time::Instant;

    const D: usize = 4;
    const P: u64 = 268369921_u64;
    const W: u64 = find_sqrt_primitive_root(D, P);

    #[test]
    fn test_ntt_neg_forward() {
        let mut values: Aligned64<[IntMod<P>; 4]> =
            Aligned64([1_u64.into(), 2_u64.into(), 3_u64.into(), 4_u64.into()]);
        let coeff_poly = IntModPoly::from(vec![1_u64, 2_u64, 3_u64, 4_u64]);

        let w = IntMod::from(W);

        ntt_neg_forward::<D, P, W>(&mut values);
        let expected = [
            coeff_poly.eval(w),
            coeff_poly.eval(w.pow(5)), // swapped, since order is bit reversed
            coeff_poly.eval(w.pow(3)),
            coeff_poly.eval(w.pow(7)),
        ];

        assert_eq!(values.0, expected);
    }

    #[test]
    fn test_ntt_neg_inverses() {
        let mut values: Aligned64<[IntMod<P>; 4]> =
            Aligned64([1_u64.into(), 2_u64.into(), 3_u64.into(), 4_u64.into()]);
        let expected = values.0;

        ntt_neg_forward::<D, P, W>(&mut values);
        ntt_neg_backward::<D, P, W>(&mut values);

        assert_eq!(values.0, expected);
    }

    #[test]
    fn test_ntt_neg_mul() {
        let mut values1: Aligned64<[IntMod<P>; 4]> =
            Aligned64([1_u64.into(), 2_u64.into(), 3_u64.into(), 4_u64.into()]);
        let mut values2: Aligned64<[IntMod<P>; 4]> =
            Aligned64([5_u64.into(), 6_u64.into(), 7_u64.into(), 8_u64.into()]);

        ntt_neg_forward::<D, P, W>(&mut values1);
        ntt_neg_forward::<D, P, W>(&mut values2);
        let mut result_points = Aligned64([
            values1.0[0] * values2.0[0],
            values1.0[1] * values2.0[1],
            values1.0[2] * values2.0[2],
            values1.0[3] * values2.0[3],
        ]);
        ntt_neg_backward::<D, P, W>(&mut result_points);

        let coeff1_poly = IntModPoly::from(vec![1_u64, 2_u64, 3_u64, 4_u64]);
        let coeff2_poly = IntModPoly::from(vec![5_u64, 6_u64, 7_u64, 8_u64]);
        let result_poly = &coeff1_poly * &coeff2_poly;

        let (result_first, result_second) = result_poly.coeff.as_slice().split_at(4);
        // x^4 = -1
        let expected: Vec<IntMod<P>> = result_first
            .iter()
            .zip(result_second.iter().chain(iter::repeat(&IntMod::zero())))
            .map(|(x, y)| x - y)
            .collect();

        assert_eq!(expected, result_points.0);
    }

    fn test_ntt_forward_size<const DD: usize, const WW: u64>() {
        let mut vec_mod = vec![];
        for i in 0..DD as u64 {
            vec_mod.push(IntMod::from(i + 1));
        }

        let mut values: Aligned64<[IntMod<P>; DD]> = Aligned64(vec_mod.clone().try_into().unwrap());
        let coeff_poly = IntModPoly::<P>::from(vec_mod);

        let w = IntMod::from(WW);

        ntt_neg_forward::<DD, P, WW>(&mut values);
        let mut expected = vec![];
        expected.resize(DD, IntMod::zero());
        for i in 0..DD {
            expected[reverse_bits::<DD>(i)] = coeff_poly.eval(w.pow(2 * (i as u64) + 1));
        }

        assert_eq!(values.0, expected.as_slice());
    }

    #[test]
    fn test_ntt_forward_8() {
        test_ntt_forward_size::<8, { find_sqrt_primitive_root(8, P) }>();
    }

    #[test]
    fn test_ntt_forward_16() {
        test_ntt_forward_size::<16, { find_sqrt_primitive_root(16, P) }>();
    }

    #[test]
    fn test_ntt_forward_32() {
        test_ntt_forward_size::<32, { find_sqrt_primitive_root(32, P) }>();
    }

    #[test]
    fn test_ntt_forward_64() {
        test_ntt_forward_size::<64, { find_sqrt_primitive_root(64, P) }>();
    }

    #[test]
    fn test_ntt_forward_128() {
        test_ntt_forward_size::<128, { find_sqrt_primitive_root(128, P) }>();
    }

    #[ignore]
    #[test]
    fn test_ntt_stress() {
        const D: usize = 2048;
        const P: u64 = 268369921;
        type RCoeff = IntModCyclo<D, P>;
        type REval = IntModCycloEval<D, P, { find_sqrt_primitive_root(D, P) }>;

        const NUM_ITER: usize = 1 << 16;

        let mut rng = ChaCha20Rng::from_entropy();
        let mut elems = Vec::with_capacity(NUM_ITER);
        for _ in 0..NUM_ITER {
            elems.push(RCoeff::rand_uniform(&mut rng));
        }
        let elems_clone = elems.clone();

        let start = Instant::now();
        for (x_expected, x_test) in elems.into_iter().zip(elems_clone.into_iter()) {
            let x_test_eval = REval::from(x_test);
            let x_test = RCoeff::from(x_test_eval);
            assert_eq!(x_expected, x_test);
        }
        let end = Instant::now();
        eprintln!(
            "took {:?} to do {} iterations ({:?} / iter)",
            end - start,
            NUM_ITER,
            (end - start) / NUM_ITER as u32
        );
    }
}
