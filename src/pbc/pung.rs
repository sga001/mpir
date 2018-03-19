use super::BatchCode;
use serde::Serialize;
use bincode::serialize;
use std::collections::HashMap;
use std::{cmp, hash};
use std::ops::{BitXor, BitXorAssign};
use super::Tuple;

pub struct PungCode<T> {
    k: usize,
    labels: HashMap<T, Vec<usize>>,
}

impl<T> PungCode<T>
where
    T: Clone + Serialize + cmp::Eq + hash::Hash,
{
    pub fn new(k: usize) -> PungCode<T> {
        assert!(k > 4, "You are better off using a subcube batch code");

        PungCode {
            k: k,
            labels: HashMap::new(),
        }
    }

    pub fn set_labels(&mut self, labels: HashMap<T, Vec<usize>>) {
        self.labels = labels;
    }
}


fn encode_bucket<T>(mut bucket: Vec<T>) -> Vec<Vec<T>>
where
    T: Clone + BitXor<Output = T>,
{
    let mut len = bucket.len();

    // split bucket (which has all the tuples) in half
    let mut bucket_2 = bucket.split_off((len + 1) / 2);

    len = bucket.len();

    // split bucket (which has half the tuples) in half again
    let bucket_1 = bucket.split_off((len + 1) / 2);

    len = bucket_2.len();

    // split bucket 2 (which has the other half of tuples) in half
    let bucket_3 = bucket_2.split_off((len + 1) / 2);

    // Now we have 4 buckets with 1/4 of the tuples in each.
    let mut encodings = vec![bucket, bucket_1, bucket_2, bucket_3];

    // Encode (XOR) collections as follows
    let plan = [(0, 1), (2, 3), (0, 2), (1, 3), (6, 7)];

    for &(c1, c2) in &plan {
        let mut bucket_i: Vec<T> = encodings[c1]
            .iter()
            .zip(&encodings[c2])
            .map(|(a, b)| a.clone() ^ b.clone())
            .collect();

        // Missing one of them due to odd number of tuples. Get it from bucket c1.
        if bucket_i.len() != encodings[c1].len() {
            bucket_i.push(encodings[c1][encodings.len() - 1].clone());
        }

        encodings.push(bucket_i);
    }

    encodings
}


impl<K, V> BatchCode<K, V> for PungCode<K>
where
    K: Clone + Serialize + BitXor<Output = K> + BitXorAssign + cmp::Eq + hash::Hash,
    V: Clone + Serialize + BitXor<Output = V> + BitXorAssign,
{
    // Encoding is placing each entry to 2 buckets (out of k).
    // Then encoding each of the k buckets with a (n, 9/4*n, 4, 9)-subcube batch code.
    // This creates a total of 9k buckets
    fn encode(&self, collection: &[Tuple<K, V>]) -> Vec<Vec<Tuple<K, V>>> {
        let mut buckets: Vec<Vec<Tuple<K, V>>> = Vec::with_capacity(self.k);

        for _ in 0..self.k {
            buckets.push(Vec::new());
        }

        for entry in collection {
            // First get the binary representation of the key
            let bytes = serialize(&entry.t.0).unwrap();

            let mut bucket_choices = Vec::with_capacity(2);

            // Map entry's key to 2 buckets (no repeats)
            for id in 0..2 {
                let mut nonce = 0;

                // The following computes bucket = sha_id(key) % k;
                let mut bucket = super::hash_and_mod(id, nonce, &bytes, self.k);

                // Ensure each key maps to *different* buckets
                while bucket_choices.contains(&bucket) {
                    nonce += 1;
                    bucket = super::hash_and_mod(id, nonce, &bytes, self.k);
                }

                bucket_choices.push(bucket);
                buckets[bucket].push(entry.clone());
            }
        }

        let total_buckets = self.k * 9;
        let mut collections: Vec<Vec<Tuple<K, V>>> = Vec::with_capacity(total_buckets);

        // Encode each bucket
        for bucket in buckets.drain(..) {
            collections.append(&mut encode_bucket(bucket));
        }

        assert_eq!(collections.len(), total_buckets);
        collections
    }

    // This implements Pung's get schedule algorithm
    // Unlike other codes, this one is state-dependent
    fn get_schedule(&self, keys: &[K]) -> Option<HashMap<K, Vec<usize>>> {
        // Make sure mapping has been initialized. This code is state-dependent.
        assert!(keys.len() <= self.labels.len());
        assert!(keys.len() <= self.k);

        let mut schedule = HashMap::new();
        let mut used = Vec::new();

        for key in keys {
            // Get index of bucket
            let mut choices: Vec<usize> = self.labels[key].clone();

            let mut bucket_choices: Vec<Vec<usize>> = Vec::new();

            for sub_bucket in choices.drain(..) {
                let offset = sub_bucket % 9; // each bucket has 9 sub buckets

                let mut entries = match offset {
                    0 => vec![
                        vec![sub_bucket],
                        vec![sub_bucket + 1, sub_bucket + 4],
                        vec![sub_bucket + 2, sub_bucket + 6],
                        vec![
                            sub_bucket + 3,
                            sub_bucket + 5,
                            sub_bucket + 7,
                            sub_bucket + 8,
                        ],
                    ],
                    1 => vec![
                        vec![sub_bucket + 1],
                        vec![sub_bucket, sub_bucket + 4],
                        vec![sub_bucket + 3, sub_bucket + 7],
                        vec![
                            sub_bucket + 2,
                            sub_bucket + 5,
                            sub_bucket + 6,
                            sub_bucket + 8,
                        ],
                    ],
                    2 => vec![
                        vec![sub_bucket + 2],
                        vec![sub_bucket + 3, sub_bucket + 5],
                        vec![sub_bucket, sub_bucket + 6],
                        vec![
                            sub_bucket + 1,
                            sub_bucket + 4,
                            sub_bucket + 7,
                            sub_bucket + 8,
                        ],
                    ],
                    3 => vec![
                        vec![sub_bucket + 3],
                        vec![sub_bucket + 2, sub_bucket + 5],
                        vec![sub_bucket + 1, sub_bucket + 7],
                        vec![sub_bucket, sub_bucket + 4, sub_bucket + 6, sub_bucket + 8],
                    ],
                    _ => continue, // Data is unencoded in the first 4 sub_buckets
                };

                bucket_choices.append(&mut entries);
            }

            let mut found = false;

            // Find a collection that has not been used.
            for mut bucket in bucket_choices.drain(..) {
                if !used.iter().any(|e| bucket.contains(e)) {
                    schedule.insert(key.clone(), bucket.clone());
                    used.append(&mut bucket);
                    found = true;
                    break;
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
        assert!(!results.is_empty() && results.len() <= 4);

        let mut decoded = results[0].clone();

        for result in results {
            decoded ^= result.clone();
        }

        decoded
    }
}
