use super::BatchCode;
use serde::Serialize;
use bincode::serialize;
use std::collections::HashMap;
use std::{cmp, hash};
use super::Tuple;
use std::ops::{BitXor, BitXorAssign};

pub struct ShardingCode {
    k: usize,
}

impl ShardingCode {
    pub fn new(k: usize) -> ShardingCode {
        assert!(k > 2, "Bound is not defined for k <= 2");
        let bound = retry_bound!(k);
        assert!(bound < k, "You are better off using replication");
        ShardingCode { k }
    }
}


impl<K, V> BatchCode<K, V> for ShardingCode
where
    K: Clone + Serialize + BitXor<Output = K> + BitXorAssign + cmp::Eq + hash::Hash,
    V: Clone + Serialize + BitXor<Output = V> + BitXorAssign,
{
    // Encoding is placing each entry in a logical bucket
    // We also replicate each logical bucket b times (b = retry bound).
    fn encode(&self, collection: &[Tuple<K, V>]) -> Vec<Vec<Tuple<K, V>>> {
        let bound = retry_bound!(self.k);

        let total_buckets = self.k * bound;

        let mut collections: Vec<Vec<Tuple<K, V>>> = Vec::with_capacity(total_buckets);

        for _ in 0..self.k {
            collections.push(Vec::new());
        }

        for entry in collection {
            // The following computes bucket = sha256(key) % k;
            let bytes = serialize(&entry.t.0).unwrap();
            let bucket = super::hash_and_mod(0, 0, &bytes, self.k);
            collections[bucket].push(entry.clone());
        }

        // Replicate each of the k logical bucket into b buckets
        // Every i mod k has the same collection where 0 <= i < b.
        for i in self.k..total_buckets {
            let clone = collections[i % self.k].clone();
            collections.push(clone);
        }

        assert_eq!(collections.len(), total_buckets);
        collections
    }

    fn get_schedule(&self, keys: &[K]) -> Option<HashMap<K, Vec<usize>>> {
        assert!(keys.len() <= self.k);
        let bound = retry_bound!(self.k);

        let mut schedule = HashMap::new();

        for key in keys {
            let bytes = serialize(&key).unwrap();
            let bucket = super::hash_and_mod(0, 0, &bytes, self.k);

            // Find a bucket that's not being used.
            for i in 0..bound {
                let entry = vec![(bucket + i * self.k)];

                if !schedule.values().any(|e| e == &entry) {
                    schedule.insert(key.clone(), entry);
                    break;
                }

                if i == bound {
                    return None;
                }
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
