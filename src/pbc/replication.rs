use super::BatchCode;
use super::Tuple;
use serde::Serialize;
use std::collections::HashMap;
use std::ops::{BitXor, BitXorAssign};
use std::{cmp, hash};

pub struct ReplicationCode {
    k: usize,
}

impl ReplicationCode {
    pub fn new(k: usize) -> ReplicationCode {
        ReplicationCode { k }
    }
}

impl<K, V> BatchCode<K, V> for ReplicationCode
where
    K: Clone + Serialize + BitXor<Output = K> + BitXorAssign + cmp::Eq + hash::Hash,
    V: Clone + Serialize + BitXor<Output = V> + BitXorAssign,
{
    fn encode(&self, collection: &[Tuple<K, V>]) -> Vec<Vec<Tuple<K, V>>> {
        let mut collections: Vec<Vec<Tuple<K, V>>> = Vec::with_capacity(self.k);
        let copy: Vec<Tuple<K, V>> = collection.to_vec();

        for _ in 0..self.k {
            collections.push(copy.clone());
        }

        collections
    }

    fn get_schedule(&self, keys: &[K]) -> Option<HashMap<K, Vec<usize>>> {
        assert!(keys.len() <= self.k);
        let mut schedule = HashMap::new();

        for (i, key) in keys.iter().enumerate() {
            schedule.insert(key.clone(), vec![i]);
        }

        Some(schedule)
    }

    fn decode(&self, results: &[Tuple<K, V>]) -> Tuple<K, V> {
        assert_eq!(results.len(), 1);
        results[0].clone()
    }
}
