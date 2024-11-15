use crate::pir::pir::{Stats, PIR};
use crate::pir::respire::Respire;
use itertools::Itertools;
use log::{info, warn};
use rand::{thread_rng, Rng};
use rayon::prelude::*;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub trait CuckooRespire: PIR {
    type BaseRespire: PIR + Respire;
    const NUM_BUCKET: usize;
}

pub struct CuckooRespireImpl<
    const BATCH_SIZE: usize,
    const NUM_BUCKET: usize,
    const NUM_RECORDS: usize,
    BaseRespire: PIR + Respire,
> {
    phantom: PhantomData<BaseRespire>,
}

impl<
        const BATCH_SIZE: usize,
        const NUM_BUCKET: usize,
        const NUM_RECORDS: usize,
        BaseRespire: PIR + Respire,
    > CuckooRespire for CuckooRespireImpl<BATCH_SIZE, NUM_BUCKET, NUM_RECORDS, BaseRespire>
{
    type BaseRespire = BaseRespire;
    const NUM_BUCKET: usize = NUM_BUCKET;
}

impl<
        const BATCH_SIZE: usize,
        const NUM_BUCKET: usize,
        const NUM_RECORDS: usize,
        BaseRespire: PIR + Respire,
    > PIR for CuckooRespireImpl<BATCH_SIZE, NUM_BUCKET, NUM_RECORDS, BaseRespire>
{
    type QueryKey = BaseRespire::QueryKey;
    type PublicParams = BaseRespire::PublicParams;

    type Query = Vec<BaseRespire::QueryOne>;

    type Response = Vec<BaseRespire::AnswerOneCompressed>;
    type Database = Vec<<BaseRespire as PIR>::Database>;
    type DatabaseHint = Vec<Vec<Option<usize>>>;
    type State = Vec<(usize, usize)>;
    type RecordBytes = BaseRespire::RecordBytes;
    const BYTES_PER_RECORD: usize = BaseRespire::BYTES_PER_RECORD;
    const NUM_RECORDS: usize = NUM_RECORDS;
    const BATCH_SIZE: usize = BATCH_SIZE;

    fn print_summary() {
        assert_eq!(BaseRespire::BATCH_SIZE, Self::NUM_BUCKET);
        eprintln!(
            "Cuckoo RESPIRE with {} bytes x {} records ({:.3} MiB)",
            BaseRespire::BYTES_PER_RECORD,
            Self::NUM_RECORDS,
            (BaseRespire::BYTES_PER_RECORD * Self::NUM_RECORDS) as f64 / 1024_f64 / 1024_f64,
        );
        eprintln!(
            "Cuckoo hashing with 3 hash functions, {} batch size, {} buckets, {} bucket size",
            Self::BATCH_SIZE,
            Self::NUM_BUCKET,
            BaseRespire::DB_SIZE,
        );
        eprintln!("Parameters (base RESPIRE): {:#?}", BaseRespire::params());
        eprintln!(
            "Public param size: {:.3} KiB",
            BaseRespire::params_public_param_size() as f64 / 1024_f64
        );
        eprintln!(
            "Query size: {:.3} KiB",
            Self::params_query_size() as f64 / 1024_f64
        );

        let (resp_size, resp_full_vecs, resp_rem) = Self::params_response_info();
        info!(
            "Response: {} record(s) => {} ring elem(s) => {} full vector(s), {} remainder",
            Self::NUM_BUCKET,
            Self::NUM_BUCKET.div_ceil(BaseRespire::PACK_RATIO_RESPONSE),
            resp_full_vecs,
            resp_rem
        );
        eprintln!(
            "Response size (batch): {:.3} KiB",
            resp_size as f64 / 1024_f64
        );

        eprintln!(
            "Record size (batch): {:.3} KiB",
            Self::params_record_size() as f64 / 1024_f64
        );
        eprintln!("Rate: {:.3}", Self::params_rate());

        eprintln!(
            "Error rate (estimated): 2^({:.3})",
            BaseRespire::params_error_rate_estimate().log2()
        )
    }

    fn encode_db<F: Fn(usize) -> Self::RecordBytes>(
        records_generator: F,
        //time_stats: Option<&mut Stats<Duration>>,
    ) -> (Self::Database, Self::DatabaseHint) {
        let begin = Instant::now();
        // TODO the bucket layouts can be determined during setup since it is database independent
        let mut bucket_layouts = vec![Vec::with_capacity(BaseRespire::DB_SIZE); Self::NUM_BUCKET];
        for i in 0..Self::NUM_RECORDS {
            let (b1, b2, b3) = Self::idx_to_buckets(i);
            bucket_layouts[b1].push(Some(i));
            bucket_layouts[b2].push(Some(i));
            bucket_layouts[b3].push(Some(i));
        }
        let max_count = bucket_layouts.iter().map(|b| b.len()).max().unwrap();
        info!(
            "Cuckoo DB encoding: worst bucket size {} out of {}",
            max_count,
            BaseRespire::DB_SIZE
        );
        if (max_count as f64 / BaseRespire::DB_SIZE as f64) < 2f64 / 3f64 {
            warn!(
                "Buckets are not very full ({} / {})",
                max_count,
                BaseRespire::DB_SIZE
            );
        }
        assert!(max_count <= BaseRespire::DB_SIZE);

        for b in bucket_layouts.iter_mut() {
            while b.len() < BaseRespire::DB_SIZE {
                b.push(None);
            }
        }

        let mut result = Vec::with_capacity(Self::NUM_BUCKET);
        let zero = Self::RecordBytes::default();
        for (b_idx, b) in bucket_layouts.iter().enumerate() {
            info!("Encoding bucket {} of {}...", b_idx + 1, Self::NUM_BUCKET);
            let bucket_records_generator =
                |i: usize| b[i].map_or(zero.clone(), |i| records_generator(i));
            result.push(BaseRespire::encode_db(bucket_records_generator).0); //, None).0);
        }

        let end = Instant::now();
        /*if let Some(time_stats) = time_stats {
            time_stats.add("encode", end - begin);
        }*/
        (result, bucket_layouts)
    }

    fn setup() -> (Self::QueryKey, Self::PublicParams) {
        //time_stats: Option<&mut Stats<Duration>>) -> (Self::QueryKey, Self::PublicParams) {
        BaseRespire::setup() //time_stats)
    }

    fn query(
        qk: &Self::QueryKey,
        record_idxs: &[usize],
        bucket_layouts: &Self::DatabaseHint,
        //mut time_stats: Option<&mut Stats<Duration>>,
    ) -> (Self::Query, Self::State) {
        let cuckoo_begin = Instant::now();
        assert_eq!(record_idxs.len(), Self::BATCH_SIZE);
        let cuckoo_mapping = Self::cuckoo(record_idxs, 2usize.pow(16)).unwrap();
        assert_eq!(cuckoo_mapping.len(), Self::BATCH_SIZE);

        let mut actual_idxs = vec![0usize; Self::NUM_BUCKET];
        for (bucket_idx, idxs_idx) in cuckoo_mapping.iter().copied() {
            let record_idx = record_idxs[idxs_idx];
            // TODO optimize this linear search
            actual_idxs[bucket_idx] = bucket_layouts[bucket_idx]
                .iter()
                .copied()
                .find_position(|slot| slot.is_some_and(|i| i == record_idx))
                .unwrap()
                .0;
        }
        let cuckoo_end = Instant::now();
        /*if let Some(time_stats) = time_stats.as_deref_mut() {
            time_stats.add("query_cuckoo", cuckoo_end - cuckoo_begin);
        }*/

        assert_eq!(actual_idxs.len(), Self::NUM_BUCKET);
        let q = actual_idxs
            .iter()
            .copied()
            .map(|idx| BaseRespire::query_one(qk, idx)) //time_stats.as_deref_mut()))
            .collect_vec();

        (q, cuckoo_mapping)
    }

    fn answer(
        pp: &Self::PublicParams,
        dbs: &Self::Database,
        qs: &Self::Query,
        qk: Option<&Self::QueryKey>,
        // mut time_stats: Option<&mut Stats<Duration>>,
    ) -> Self::Response {
        assert_eq!(qs.len(), Self::NUM_BUCKET);
        let answers: Vec<BaseRespire::AnswerOne> = (*qs)
            .par_iter()
            .zip((*dbs).par_iter())
            .map(|(q, db)| BaseRespire::answer_one(pp, db, q, qk)) //time_stats.as_deref_mut()))
            .collect(); //.collect_vec();
        let answers_compressed = answers
            .chunks(BaseRespire::RESPONSE_CHUNK_SIZE)
            .map(|chunk| {
                BaseRespire::answer_compress_chunk(pp, chunk, qk) //time_stats.as_deref_mut())
            })
            .collect_vec();
        answers_compressed
    }

    fn extract(
        qk: &Self::QueryKey,
        r: &Self::Response,
        cuckoo_mapping: &Self::State,
        // mut time_stats: Option<&mut Stats<Duration>>,
    ) -> Vec<Self::RecordBytes> {
        let mut result_by_bucket = Vec::with_capacity(Self::NUM_BUCKET);
        for r_one in r {
            let extracted = BaseRespire::extract_one(qk, r_one); //time_stats.as_deref_mut());
            for record in extracted {
                if result_by_bucket.len() < Self::NUM_BUCKET {
                    result_by_bucket.push(record);
                }
            }
        }
        assert_eq!(result_by_bucket.len(), Self::NUM_BUCKET);

        let uncuckoo_begin = Instant::now();
        let mut result = vec![BaseRespire::RecordBytes::default(); Self::BATCH_SIZE];
        assert_eq!(cuckoo_mapping.len(), Self::BATCH_SIZE);
        for (bucket_idx, idxs_idx) in cuckoo_mapping.iter().copied() {
            result[idxs_idx] = result_by_bucket[bucket_idx].clone();
        }
        let uncuckoo_end = Instant::now();
        /*if let Some(time_stats) = time_stats {
            time_stats.add("extract_uncuckoo", uncuckoo_end - uncuckoo_begin);
        }*/
        result
    }
}

impl<
        const BATCH_SIZE: usize,
        const NUM_BUCKET: usize,
        const NUM_RECORDS: usize,
        BaseRespire: PIR + Respire,
    > CuckooRespireImpl<BATCH_SIZE, NUM_BUCKET, NUM_RECORDS, BaseRespire>
{
    fn idx_to_buckets(i: usize) -> (usize, usize, usize) {
        let modulus = Self::NUM_BUCKET as u64;
        assert!(modulus.checked_pow(3).is_some());
        // TODO: DefaultHasher is not stable
        let mut hasher = DefaultHasher::new();
        i.hash(&mut hasher);
        let hashed = hasher.finish();
        let h1 = hashed % modulus;
        let h2 = (hashed / modulus) % modulus;
        let h3 = (hashed / modulus / modulus) % modulus;
        (h1 as usize, h2 as usize, h3 as usize)
    }

    ///
    /// Returns a vector of (bucket slot index, item index) pairs.
    ///
    fn cuckoo(items: &[usize], max_depth: usize) -> Option<Vec<(usize, usize)>> {
        // Maps bucket slot indices to item indices
        let mut mapping = HashMap::with_capacity(items.len());
        let mut remaining = Vec::from_iter((0..items.len()).map(|idx| (idx, 0usize)));
        let mut rng = thread_rng();
        while let Some((idx, depth)) = remaining.pop() {
            if depth >= max_depth {
                return None;
            }
            let (i1, i2, i3) = Self::idx_to_buckets(items[idx]);
            match (mapping.get(&i1), mapping.get(&i2), mapping.get(&i3)) {
                (None, _, _) => {
                    mapping.insert(i1, idx);
                }
                (_, None, _) => {
                    mapping.insert(i2, idx);
                }
                (_, _, None) => {
                    mapping.insert(i3, idx);
                }
                (Some(&curr1), Some(&curr2), Some(&curr3)) => match rng.gen_range(0..3) {
                    0 => {
                        remaining.push((curr1, depth + 1));
                        mapping.insert(i1, idx);
                    }
                    1 => {
                        remaining.push((curr2, depth + 1));
                        mapping.insert(i2, idx);
                    }
                    _ => {
                        remaining.push((curr3, depth + 1));
                        mapping.insert(i3, idx);
                    }
                },
            }
        }
        Some(mapping.into_iter().collect_vec())
    }

    pub fn params_query_size() -> usize {
        Self::NUM_BUCKET * BaseRespire::params_query_one_size()
    }

    pub fn params_record_size() -> usize {
        Self::BATCH_SIZE * BaseRespire::params_record_one_size()
    }

    ///
    /// size, number of full vectors, remainder size
    ///
    pub fn params_response_info() -> (usize, usize, usize) {
        let num_ring_elem = Self::NUM_BUCKET.div_ceil(BaseRespire::PACK_RATIO_RESPONSE);
        let num_full_vecs = num_ring_elem / BaseRespire::N_VEC;
        let num_rem = num_ring_elem % BaseRespire::N_VEC;

        let full_vec_size = BaseRespire::params_response_one_size(BaseRespire::N_VEC);
        let rem_vec_size = if num_rem > 0 {
            BaseRespire::params_response_one_size(num_rem)
        } else {
            0
        };
        (
            num_full_vecs * full_vec_size + rem_vec_size,
            num_full_vecs,
            num_rem,
        )
    }

    pub fn params_rate() -> f64 {
        (Self::params_record_size() as f64) / (Self::params_response_info().0 as f64)
    }
}
