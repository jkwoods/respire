//! RingGSW with both NTT and CRT. The only scheme that should be used in practice due to its efficiency and noise budget.

use crate::fhe::fhe::*;
use crate::fhe::gsw_utils::*;
use crate::math::int_mod_cyclo::IntModCyclo;
use crate::math::int_mod_cyclo_crt::IntModCycloCRT;
use crate::math::int_mod_cyclo_crt_eval::IntModCycloCRTEval;
use crate::math::matrix::Matrix;
use crate::math::utils::ceil_log;

pub struct RingGSWNTTCRT<
    const N_MINUS_1: usize,
    const N: usize,
    const M: usize,
    const P: u64,
    const Q: u64,
    const Q1: u64,
    const Q2: u64,
    const D: usize,
    const G_BASE: u64,
    const G_LEN: usize,
    const NOISE_WIDTH_MILLIONTHS: u64,
> {}

#[derive(Clone, Debug)]
pub struct RingGSWNTTCRTCiphertext<
    const N: usize,
    const M: usize,
    const P: u64,
    const Q: u64,
    const Q1: u64,
    const Q2: u64,
    const D: usize,
    const G_BASE: u64,
    const G_LEN: usize,
> {
    ct: Matrix<N, M, IntModCycloCRTEval<D, Q1, Q2>>,
}

#[derive(Clone, Debug)]
pub struct RingGSWNTTCRTPublicKey<
    const N: usize,
    const M: usize,
    const P: u64,
    const Q: u64,
    const Q1: u64,
    const Q2: u64,
    const D: usize,
    const G_BASE: u64,
    const G_LEN: usize,
> {
    A: Matrix<N, M, IntModCycloCRTEval<D, Q1, Q2>>,
}

#[derive(Clone, Debug)]
pub struct RingGSWNTTCRTSecretKey<
    const N: usize,
    const M: usize,
    const P: u64,
    const Q: u64,
    const Q1: u64,
    const Q2: u64,
    const D: usize,
    const G_BASE: u64,
    const G_LEN: usize,
> {
    s_T: Matrix<1, N, IntModCycloCRTEval<D, Q1, Q2>>,
}

impl<
        const N_MINUS_1: usize,
        const N: usize,
        const M: usize,
        const P: u64,
        const Q: u64,
        const Q1: u64,
        const Q2: u64,
        const D: usize,
        const G_BASE: u64,
        const G_LEN: usize,
        const NOISE_WIDTH_MILLIONTHS: u64,
    > FHEScheme
    for RingGSWNTTCRT<N_MINUS_1, N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN, NOISE_WIDTH_MILLIONTHS>
{
}

impl<
        const N_MINUS_1: usize,
        const N: usize,
        const M: usize,
        const P: u64,
        const Q: u64,
        const Q1: u64,
        const Q2: u64,
        const D: usize,
        const G_BASE: u64,
        const G_LEN: usize,
        const NOISE_WIDTH_MILLIONTHS: u64,
    > EncryptionScheme
    for RingGSWNTTCRT<N_MINUS_1, N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN, NOISE_WIDTH_MILLIONTHS>
{
    type Plaintext = IntModCyclo<D, P>;
    type Ciphertext = RingGSWNTTCRTCiphertext<N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN>;
    type PublicKey = RingGSWNTTCRTPublicKey<N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN>;
    type SecretKey = RingGSWNTTCRTSecretKey<N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN>;

    fn keygen() -> (Self::PublicKey, Self::SecretKey) {
        let (A, s_T) = gsw_keygen::<N_MINUS_1, N, M, _, NOISE_WIDTH_MILLIONTHS>();
        (Self::PublicKey { A }, Self::SecretKey { s_T })
    }

    fn encrypt(pk: &Self::PublicKey, mu: &Self::Plaintext) -> Self::Ciphertext {
        let mu1: IntModCyclo<D, Q1> = mu.include_into();
        let mu2: IntModCyclo<D, Q2> = mu.include_into();
        let mu = (&IntModCycloCRT::from((mu1, mu2))).into();
        let ct = gsw_encrypt_pk::<N, M, G_BASE, G_LEN, _>(&pk.A, mu);
        Self::Ciphertext { ct }
    }

    fn encrypt_sk(sk: &Self::SecretKey, mu: &Self::Plaintext) -> Self::Ciphertext {
        let mu1: IntModCyclo<D, Q1> = mu.include_into();
        let mu2: IntModCyclo<D, Q2> = mu.include_into();
        let mu = (&IntModCycloCRT::from((mu1, mu2))).into();
        let ct = gsw_encrypt_sk::<N_MINUS_1, N, M, G_BASE, G_LEN, _, NOISE_WIDTH_MILLIONTHS>(
            &sk.s_T, mu,
        );
        Self::Ciphertext { ct }
    }

    fn decrypt(sk: &Self::SecretKey, ct: &Self::Ciphertext) -> Self::Plaintext {
        let s_T = &sk.s_T;
        let ct = &ct.ct;
        let pt_eval = gsw_half_decrypt::<N, M, P, Q, G_BASE, G_LEN, _>(s_T, ct);
        let pt: IntModCycloCRT<D, Q1, Q2> = pt_eval.into();
        pt.round_down_into()
    }
}

/*
 * RingGSWNTTCRT homomorphic addition / multiplication
 */

impl<
        const N_MINUS_1: usize,
        const N: usize,
        const M: usize,
        const P: u64,
        const Q: u64,
        const Q1: u64,
        const Q2: u64,
        const D: usize,
        const G_BASE: u64,
        const G_LEN: usize,
        const NOISE_WIDTH_MILLIONTHS: u64,
    > AddHomEncryptionScheme
    for RingGSWNTTCRT<N_MINUS_1, N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN, NOISE_WIDTH_MILLIONTHS>
{
    fn add_hom(lhs: &Self::Ciphertext, rhs: &Self::Ciphertext) -> Self::Ciphertext {
        Self::Ciphertext {
            ct: ciphertext_add::<N, M, G_BASE, G_LEN, _>(&lhs.ct, &rhs.ct),
        }
    }
}

impl<
        const N_MINUS_1: usize,
        const N: usize,
        const M: usize,
        const P: u64,
        const Q: u64,
        const Q1: u64,
        const Q2: u64,
        const D: usize,
        const G_BASE: u64,
        const G_LEN: usize,
        const NOISE_WIDTH_MILLIONTHS: u64,
    > MulHomEncryptionScheme
    for RingGSWNTTCRT<N_MINUS_1, N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN, NOISE_WIDTH_MILLIONTHS>
{
    fn mul_hom(lhs: &Self::Ciphertext, rhs: &Self::Ciphertext) -> Self::Ciphertext {
        Self::Ciphertext {
            ct: ciphertext_mul::<N, M, G_BASE, G_LEN, _>(&lhs.ct, &rhs.ct),
        }
    }
}

impl<
        const N_MINUS_1: usize,
        const N: usize,
        const M: usize,
        const P: u64,
        const Q: u64,
        const Q1: u64,
        const Q2: u64,
        const D: usize,
        const G_BASE: u64,
        const G_LEN: usize,
        const NOISE_WIDTH_MILLIONTHS: u64,
    > AddScalarEncryptionScheme<IntModCyclo<D, P>>
    for RingGSWNTTCRT<N_MINUS_1, N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN, NOISE_WIDTH_MILLIONTHS>
{
    fn add_scalar(lhs: &Self::Ciphertext, rhs: &Self::Plaintext) -> Self::Ciphertext {
        let rhs_q1 = rhs.include_into();
        let rhs_q2 = rhs.include_into();
        let rhs_q: IntModCycloCRT<D, Q1, Q2> = (rhs_q1, rhs_q2).into();
        let rhs_q_eval = rhs_q.into();
        Self::Ciphertext {
            ct: scalar_ciphertext_add::<N, M, G_BASE, G_LEN, _>(&lhs.ct, &rhs_q_eval),
        }
        // // TODO: see below, not fast
        // let rhs_q = &Z_N_CycloNTT_CRT::<D, Q1, Q2, W1, W2>::from(u64::from(*rhs));
        // Ciphertext {
        //     // ct: &self.ct + &(&build_gadget::<Z_N_CycloNTT_CRT::<D, Q1, Q2, W1, W2>, N, M, G_BASE, G_LEN>() * rhs_q),
        //     ct: &self.ct + &(&build_gadget::<Z_N_CycloNTT_CRT::<D, Q1, Q2, W1, W2>, N, M, G_BASE, G_LEN>() * rhs_q),
    }
}

impl<
        const N_MINUS_1: usize,
        const N: usize,
        const M: usize,
        const P: u64,
        const Q: u64,
        const Q1: u64,
        const Q2: u64,
        const D: usize,
        const G_BASE: u64,
        const G_LEN: usize,
        const NOISE_WIDTH_MILLIONTHS: u64,
    > MulScalarEncryptionScheme<IntModCyclo<D, P>>
    for RingGSWNTTCRT<N_MINUS_1, N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN, NOISE_WIDTH_MILLIONTHS>
{
    fn mul_scalar(lhs: &Self::Ciphertext, rhs: &Self::Plaintext) -> Self::Ciphertext {
        let rhs_q1 = rhs.include_into();
        let rhs_q2 = rhs.include_into();
        let rhs_q: IntModCycloCRT<D, Q1, Q2> = (rhs_q1, rhs_q2).into();
        let rhs_q_eval = rhs_q.into();
        Self::Ciphertext {
            ct: scalar_ciphertext_mul::<N, M, G_BASE, G_LEN, _>(&lhs.ct, &rhs_q_eval),
        }
        // let rhs_q = &Z_N_CycloNTT_CRT::<D, Q1, Q2, W1, W2>::from(u64::from(rhs));
        // Ciphertext {
        //     ct: &self.ct
        //         * &gadget_inverse::<_, N, M, M, G_BASE, G_LEN>(
        //             &(&build_gadget::<Z_N_CycloNTT_CRT::<D, Q1, Q2, W1, W2>, N, M, G_BASE, G_LEN>() * rhs_q),
        //         ),
        // }

        // TODO: below is the faster way to do it, need to add arbitrary scalar * matrix mult
        // let rhs_q = Z_N_CRT::from(u64::from(rhs));
        // let mut G_rhs: Matrix<N, M, Z_N_CycloRaw_CRT<D, Q1, Q2>> =
        //     build_gadget::<Z_N_CycloRaw_CRT<D, Q1, Q2>, N, M, G_BASE, G_LEN>();
        // for i in 0..N {
        //     for j in 0..M {
        //         G_rhs[(i, j)] *= rhs_q;
        //     }
        // }

        // let G_inv_G_rhs_raw: Matrix<M, M, Z_N_CycloRaw_CRT<D, Q1, Q2>> =
        //     gadget_inverse::<Z_N_CycloRaw_CRT<D, Q1, Q2>, N, M, M, G_BASE, G_LEN>(&G_rhs);

        // let mut G_inv_G_rhs_ntt: Matrix<M, M, Z_N_CycloNTT_CRT<D, Q1, Q2, W1, W2>> = Matrix::zero();
        // for i in 0..M {
        //     for j in 0..M {
        //         G_inv_G_rhs_ntt[(i, j)] = (&G_inv_G_rhs_raw[(i, j)]).into();
        //     }
        // }

        // Ciphertext {
        //     ct: &self.ct
        //         * &gadget_inverse::<
        //             Z_N_CycloNTT_CRT<D, Q1, Q2, W1, W2>,
        //             N,
        //             M,
        //             M,
        //             G_BASE,
        //             G_LEN,
        //         >(
        //             &(&build_gadget::<
        //                 Z_N_CycloNTT_CRT<D, Q1, Q2, W1, W2>,
        //                 N,
        //                 M,
        //                 G_BASE,
        //                 G_LEN,
        //             >() * rhs_q),
        //         ),
        // }
    }
}

impl<
        const N_MINUS_1: usize,
        const N: usize,
        const M: usize,
        const P: u64,
        const Q: u64,
        const Q1: u64,
        const Q2: u64,
        const D: usize,
        const G_BASE: u64,
        const G_LEN: usize,
        const NOISE_WIDTH_MILLIONTHS: u64,
    > NegEncryptionScheme
    for RingGSWNTTCRT<N_MINUS_1, N, M, P, Q, Q1, Q2, D, G_BASE, G_LEN, NOISE_WIDTH_MILLIONTHS>
{
    fn negate(ct: &Self::Ciphertext) -> Self::Ciphertext {
        Self::Ciphertext { ct: -&ct.ct }
    }
}

/*
 * RingGSWNTTCRT params
 */

pub struct Params {
    pub N: usize,
    pub M: usize,
    pub P: u64,
    pub Q1: u64,
    pub Q2: u64,
    pub D: usize,
    pub G_BASE: u64,
    pub NOISE_WIDTH_MILLIONTHS: u64,
}

macro_rules! gsw_from_params {
    ($params:expr) => {
        RingGSWNTTCRT<
            { $params.N - 1 },
            { $params.N },
            { $params.M },
            { $params.P },
            { $params.Q1 * $params.Q2 },
            { $params.Q1 },
            { $params.Q2 },
            { $params.D },
            { $params.G_BASE },
            { ceil_log($params.G_BASE, $params.Q1 * $params.Q2) },
            { $params.NOISE_WIDTH_MILLIONTHS },
        >
    }
}

/*
 * Pre-defined sets of parameters
 */

// TODO? Tests passed when I set W1, W2 = 1, but maybe this is only because of constants...
pub const RING_GSW_NTT_CRT_TEST_PARAMS: Params = Params {
    N: 2,
    M: 112,
    P: 31,
    Q1: 268369921,
    Q2: 249561089,
    D: 4,
    G_BASE: 2,
    NOISE_WIDTH_MILLIONTHS: 6_400_000,
};
pub const RING_GSW_NTT_CRT_TEST_MEDIUM_PARAMS: Params = Params {
    N: 2,
    M: 8,
    P: 31,
    Q1: 268369921,
    Q2: 249561089,
    D: 2048,
    // D: 256,
    G_BASE: 16088,
    NOISE_WIDTH_MILLIONTHS: 6_400_000,
};

pub type RingGSWNTTCRTTest = gsw_from_params!(RING_GSW_NTT_CRT_TEST_PARAMS);
pub type RingGSWNTTCRTTestMedium = gsw_from_params!(RING_GSW_NTT_CRT_TEST_MEDIUM_PARAMS);

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn keygen_is_correct() {
        let threshold =
            4f64 * (RING_GSW_NTT_CRT_TEST_PARAMS.NOISE_WIDTH_MILLIONTHS as f64 / 1_000_000_f64);
        let (A, s_T) = RingGSWNTTCRTTest::keygen();
        let e = &s_T.s_T * &A.A;

        for i in 0..RING_GSW_NTT_CRT_TEST_PARAMS.M {
            assert!(
                (e[(0, i)].norm() as f64) < threshold,
                "e^T = s_T * A was too big"
            );
        }
    }

    #[test]
    fn encryption_is_correct() {
        let (A, s_T) = RingGSWNTTCRTTest::keygen();
        for i in 0_u64..10_u64 {
            let mu = IntModCyclo::from(i);
            let ct = RingGSWNTTCRTTest::encrypt(&A, &mu);
            let pt = RingGSWNTTCRTTest::decrypt(&s_T, &ct);
            assert_eq!(pt, mu, "decryption failed");
        }
    }

    #[test]
    fn homomorphism_is_correct() {
        let (A, s_T) = RingGSWNTTCRTTest::keygen();
        for i in 0_u64..10_u64 {
            for j in 0_u64..10_u64 {
                let mu1 = IntModCyclo::from(i);
                let mu2 = IntModCyclo::from(j);
                let ct1 = RingGSWNTTCRTTest::encrypt(&A, &mu1);
                let ct2 = RingGSWNTTCRTTest::encrypt(&A, &mu2);

                let pt_add_ct =
                    RingGSWNTTCRTTest::decrypt(&s_T, &(RingGSWNTTCRTTest::add_hom(&ct1, &ct2)));
                let pt_mul_ct =
                    RingGSWNTTCRTTest::decrypt(&s_T, &(RingGSWNTTCRTTest::mul_hom(&ct1, &ct2)));
                let pt_mul_scalar =
                    RingGSWNTTCRTTest::decrypt(&s_T, &(RingGSWNTTCRTTest::mul_scalar(&ct1, &mu2)));

                assert_eq!(pt_add_ct, &mu1 + &mu2, "ciphertext addition failed");
                assert_eq!(pt_mul_ct, &mu1 * &mu2, "ciphertext multiplication failed");
                assert_eq!(
                    pt_mul_scalar,
                    &mu1 * &mu2,
                    "multiplication by scalar failed"
                );
            }
        }
    }

    #[test]
    fn homomorphism_mul_multiple_correct() {
        let (A, s_T) = RingGSWNTTCRTTest::keygen();
        let mu1 = IntModCyclo::from(5_u64);
        let mu2 = IntModCyclo::from(12_u64);
        let mu3 = IntModCyclo::from(6_u64);
        let mu4 = IntModCyclo::from(18_u64);

        let ct1 = RingGSWNTTCRTTest::encrypt(&A, &mu1);
        let ct2 = RingGSWNTTCRTTest::encrypt(&A, &mu2);
        let ct3 = RingGSWNTTCRTTest::encrypt(&A, &mu3);
        let ct4 = RingGSWNTTCRTTest::encrypt(&A, &mu4);

        let ct12 = RingGSWNTTCRTTest::mul_hom(&ct1, &ct2);
        let ct34 = RingGSWNTTCRTTest::mul_hom(&ct3, &ct4);
        let ct1234 = RingGSWNTTCRTTest::mul_hom(&ct12, &ct34);
        // let ct31234 = &ct3 * &ct1234;

        let pt12 = RingGSWNTTCRTTest::decrypt(&s_T, &ct12);
        let pt34 = RingGSWNTTCRTTest::decrypt(&s_T, &ct34);
        let pt1234 = RingGSWNTTCRTTest::decrypt(&s_T, &ct1234);
        // let pt31234 = gsw::decrypt(&s_T, &ct31234);

        assert_eq!(pt12, &mu1 * &mu2);
        assert_eq!(pt34, &mu3 * &mu4);
        assert_eq!(pt1234, &(&(&mu1 * &mu2) * &mu3) * &mu4);
        // assert_eq!(pt31234, &(&(&(&mu1 * &mu2) * &mu3) * &mu4) * &mu3);
    }
}