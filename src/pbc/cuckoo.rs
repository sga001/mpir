use super::BatchCode;
use serde::Serialize;
use bincode::serialize;
use std::collections::HashMap;
use std::{cmp, hash};
use rand;
use rand::Rng;
use super::Tuple;
use std::ops::{BitXor, BitXorAssign};

const MAX_ATTEMPTS: usize = 1000;

pub struct CuckooCode {
    k: usize,
    d: usize, // d choices
    r: f64,   // total buckets = ceil(k * r)
}

impl CuckooCode {
    pub fn new(k: usize, d: usize, r: f64) -> CuckooCode {
        CuckooCode { k, d, r }
    }
}

// Cuckoo hashing insert algorithm (recursive!).
// Algborithm: if either of d buckets is empty, insert there.
// Otherwise choose one of them at random, insert item there,
// and relocate existing element by running insert algorithm.
fn insert<K>(
    elements: &mut HashMap<usize, K>,
    buckets: &HashMap<K, Vec<usize>>,
    key: &K,
    attempt: usize,
    rng: &mut Rng,
) -> bool
where
    K: Clone + Serialize + cmp::Eq + hash::Hash,
{
    if attempt >= MAX_ATTEMPTS {
        return false;
    }

    // Case 1: check to see if any of the d buckets is empty. If so, insert there.
    for hash_id in &buckets[key] {
        if !elements.contains_key(hash_id) {
            elements.insert(*hash_id, key.clone());
            return true;
        }
    }

    // Case 2: all possible buckets are filled. Relocate an existing entry

    let possible_buckets = &buckets[key];
    let index = (rng.next_u32() as usize) % possible_buckets.len();
    let chosen_bucket = possible_buckets[index];

    // Insert new key, and get the key that was previously inserted
    let old_key = elements.insert(chosen_bucket, key.clone()).unwrap();

    // Re-insert the key that we're relocating
    insert(elements, buckets, &old_key, attempt + 1, rng)
}

impl<K, V> BatchCode<K, V> for CuckooCode
where
    K: Clone + Serialize + BitXor<Output = K> + BitXorAssign + cmp::Eq + hash::Hash,
    V: Clone + Serialize + BitXor<Output = V> + BitXorAssign,
{
    // Encoding is placing each entry to d buckets
    // This is very different from standard cuckoo hashing.
    fn encode(&self, collection: &[Tuple<K, V>]) -> Vec<Vec<Tuple<K, V>>> {
        let total_buckets = (self.k as f64 * self.r).ceil() as usize;
        let mut collections: Vec<Vec<Tuple<K, V>>> = Vec::with_capacity(total_buckets);

        for _ in 0..total_buckets {
            collections.push(Vec::new());
        }

        for entry in collection {
            // First get the binary representation of the key
            let bytes = serialize(&entry.t.0).unwrap();

            let mut bucket_choices = Vec::with_capacity(self.d);

            // Map entry's key to d buckets (no repeats)
            for id in 0..self.d {
                let mut nonce = 0;

                // The following computes bucket = sha_d(key) % total_buckets
                let mut bucket = super::hash_and_mod(id, nonce, &bytes, total_buckets);

                // Ensure each key maps to *different* buckets
                while bucket_choices.contains(&bucket) {
                    nonce += 1;
                    bucket = super::hash_and_mod(id, nonce, &bytes, total_buckets);
                }

                bucket_choices.push(bucket);
                collections[bucket].push(entry.clone());
            }
        }

        collections
    }

    fn get_schedule(&self, keys: &[K]) -> Option<HashMap<K, Vec<usize>>> {
        assert!(keys.len() <= self.k);

        let total_buckets = (self.k as f64 * self.r).ceil() as usize;

        let mut buckets = HashMap::new(); // map containing K -> [bucket 1, ..., bucket d]

        for key in keys {
            let bytes = serialize(&key).unwrap();
            let mut bucket_choices = Vec::with_capacity(self.d);

            // Map entry's key to d buckets (no repeats)
            for id in 0..self.d {
                let mut nonce = 0;

                // The following computes bucket = sha_d(key) % k;
                let mut bucket = super::hash_and_mod(id, nonce, &bytes, total_buckets);

                // Ensure each key maps to *different* buckets
                while bucket_choices.contains(&bucket) {
                    nonce += 1;
                    bucket = super::hash_and_mod(id, nonce, &bytes, total_buckets);
                }

                bucket_choices.push(bucket);
            }

            buckets.insert(key, bucket_choices);
        }

        // This is a variant of the Insert algorithm in cuckoo hashing (Pagh and Rodler).
        // One difference is we only have 1 table and d hash functions.
        // The difference is that we are doing this for retrieval rather than insertion! (the keys
        // have already been inserted). What this means is that cuckoo hashing is being applied
        // with respect to the client's keys (not the keys that the storage server received).
        // This is a crucial but subtle difference.

        let mut rng = rand::thread_rng();
        let mut elements = HashMap::new(); // map containing bucket -> [current key]

        for key in keys {
            if !insert(&mut elements, &buckets, &key, 0, &mut rng) {
                return None;
            }
        }

        let mut schedule = HashMap::new();

        for (k, v) in elements {
            schedule.insert(v.clone(), vec![k]);
        }

        assert_eq!(schedule.len(), keys.len());

        Some(schedule)
    }

    fn decode(&self, results: &[Tuple<K, V>]) -> Tuple<K, V> {
        assert_eq!(results.len(), 1);
        results[0].clone()
    }
}
