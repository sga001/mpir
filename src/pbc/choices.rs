use super::BatchCode;
use serde::Serialize;
use bincode::serialize;
use std::collections::HashMap;
use std::{cmp, hash};
use super::Tuple;
use std::ops::{BitXor, BitXorAssign};

pub struct ChoicesCode {
    k: usize,
    d: usize, // d choices
}

impl ChoicesCode {
    pub fn new(k: usize, d: usize) -> ChoicesCode {
        let bound = retry_bound!(k, d);
        assert!(bound < k, "You are better off using replication");
        ChoicesCode { k, d }
    }
}

impl<K, V> BatchCode<K, V> for ChoicesCode
where
    K: Clone + Serialize + BitXor<Output = K> + BitXorAssign + cmp::Eq + hash::Hash,
    V: Clone + Serialize + BitXor<Output = V> + BitXorAssign,
{
    // Encoding is placing each entry to d logical buckets.
    // We also replicate each logical bucket b times (b = retry bound).
    fn encode(&self, collection: &[Tuple<K, V>]) -> Vec<Vec<Tuple<K, V>>> {
        let bound = retry_bound!(self.k, self.d);

        let total_buckets = self.k * bound;
        let mut collections: Vec<Vec<Tuple<K, V>>> = Vec::with_capacity(total_buckets);

        for _ in 0..self.k {
            collections.push(Vec::new());
        }

        for entry in collection {
            // First get the binary representation of the key
            let bytes = serialize(&entry.t.0).unwrap();

            let mut bucket_choices = Vec::with_capacity(self.d);

            // Map entry's key to d buckets (no repeats)
            for id in 0..self.d {
                let mut nonce = 0;

                // The following computes bucket = sha_d(key) % k;
                let mut bucket = super::hash_and_mod(id, nonce, &bytes, self.k);

                // Ensure each key maps to *different* buckets
                while bucket_choices.contains(&bucket) {
                    nonce += 1;
                    bucket = super::hash_and_mod(id, nonce, &bytes, self.k);
                }

                bucket_choices.push(bucket);
                collections[bucket].push(entry.clone());
            }
        }

        // Replicate each of the k logical buckets into b buckets
        // Every i mod k has the same collection, where 0 <= i < b.
        for i in self.k..total_buckets {
            let clone = collections[i % self.k].clone();
            collections.push(clone);
        }


        assert_eq!(collections.len(), total_buckets);
        collections
    }

    // This is an adaptation of the "Greedy" algorithm of Azar et al.'s
    // Balanced allocations paper, STOC '94.
    // The difference is that for each of the k buckets, we have b replicas.
    // If two balls map to the same bucket in Greedy, it just places the 2 balls in the same bucket.
    // In our case, if 2 balls map to the same bucket, we place each ball in a different
    // replicas of each bucket.
    // This is different from Po2C in that we are doing this for retrieval rather than
    // storage. What this means is that Po2C is being applied with respect to the
    // client's keys (not the keys that the storage server received!). This is a crucial
    // but subtle difference.
    fn get_schedule(&self, keys: &[K]) -> Option<HashMap<K, Vec<usize>>> {
        assert!(keys.len() <= self.k);
        let bound = retry_bound!(self.k, self.d);
        let mut schedule = HashMap::new();

        for key in keys {
            let bytes = serialize(&key).unwrap();
            let mut bucket_choices = Vec::with_capacity(self.d);
            let mut found = false;

            // Map entry's key to d buckets (no repeats)
            for id in 0..self.d {
                let mut nonce = 0;

                // The following computes bucket = sha_d(key) % k;
                let mut bucket = super::hash_and_mod(id, nonce, &bytes, self.k);

                // Ensure each key maps to *different* buckets
                while bucket_choices.contains(&bucket) {
                    nonce += 1;
                    bucket = super::hash_and_mod(id, nonce, &bytes, self.k);
                }

                bucket_choices.push(bucket);
            }

            // Find a bucket that has not been used. This is sort of analogous
            // to Greedy, but not quite.
            'bucket_loop: for bucket in bucket_choices {
                for i in 0..bound {
                    let entry = vec![bucket + i * self.k];

                    if !schedule.values().any(|e| e == &entry) {
                        schedule.insert(key.clone(), entry);
                        found = true;
                        break 'bucket_loop;
                    }
                }
            }

            if !found {
                return None;
            }
        }

        // maps each index into a vector of 1 entry containing that index
        // We do this only to meet the return type: each entry represents the
        // set of indices that must be queried (in our case, our encoding is
        // systematic so we don't need to have multiple indices per item).
        Some(schedule)
    }

    fn decode(&self, results: &[Tuple<K, V>]) -> Tuple<K, V> {
        assert_eq!(results.len(), 1);
        results[0].clone()
    }
}
