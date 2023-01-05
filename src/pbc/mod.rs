use crypto::digest::Digest;
use crypto::sha2::Sha256;
use num::bigint::BigUint;
use num::cast::ToPrimitive;
use num::Integer;
use serde::Serialize;
use std::collections::HashMap;
use std::ops::{BitXor, BitXorAssign};
use std::{cmp, hash};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Tuple<K, V>
where
    K: BitXor + BitXorAssign + Clone + Serialize,
    V: BitXor + BitXorAssign + Clone + Serialize,
{
    pub t: (K, V),
}

impl<K, V> BitXor for Tuple<K, V>
where
    K: BitXor<Output = K> + BitXorAssign + Clone + Serialize,
    V: BitXor<Output = V> + BitXorAssign + Clone + Serialize,
{
    type Output = Tuple<K, V>;

    fn bitxor(self, rhs: Tuple<K, V>) -> Tuple<K, V> {
        Tuple {
            t: (self.t.0 ^ rhs.t.0, self.t.1 ^ rhs.t.1),
        }
    }
}

impl<K, V> BitXorAssign for Tuple<K, V>
where
    K: BitXor<Output = K> + BitXorAssign + Clone + Serialize,
    V: BitXor<Output = V> + BitXorAssign + Clone + Serialize,
{
    fn bitxor_assign(&mut self, other: Tuple<K, V>) {
        self.t.0 ^= other.t.0;
        self.t.1 ^= other.t.1;
    }
}

pub trait BatchCode<K, V>
where
    K: Clone + Serialize + BitXor<Output = K> + BitXorAssign + cmp::Eq + hash::Hash,
    V: Clone + Serialize + BitXor<Output = V> + BitXorAssign,
{
    /// Encodes a collection into m collections such that k items can be
    /// retrieved by querying each of the m collections at most once (with high prob).
    /// This is typically called by the server.
    fn encode(&self, collection: &[Tuple<K, V>]) -> Vec<Vec<Tuple<K, V>>>;

    /// This function takes as input a set of keys and returns a possible schedule (i.e., which
    /// collection or collections to get each key from), or None if no such schedule can be found.
    /// Note that this does not mean that the key exists in the collections. It only means that
    /// if the key were to exist, it would be found in those collections.
    /// This function is typically called by the client.
    ///
    /// WARNING: Assumes unique keys (this is not fundamental and does not change the performance,
    /// but we're using a hashmap as output so duplicate keys will be overwritten...).
    fn get_schedule(&self, keys: &[K]) -> Option<HashMap<K, Vec<usize>>>;

    /// This function takes a vector of tuples and combines them together into the
    /// desired tuple. In many cases, the vector contains only one entry in which case it is
    /// the result (K, V). In other cases, XORing or some other operation is performed.
    /// This function is typically called by the client
    fn decode(&self, results: &[Tuple<K, V>]) -> Tuple<K, V>;
}

#[macro_export]
macro_rules! retry_bound {
    ($k:expr) => {
        3 * (($k as f64).ln() / ($k as f64).ln().ln()).ceil() as usize
    };

    ($k:expr, $d:expr) => {
        1 + ((($k as f64).ln().ln() / ($d as f64).ln()) + 1.0).ceil() as usize
    };
}

// utility function that computes a hash of the key and mods it by the given modulus
fn hash_and_mod(id: usize, nonce: usize, data: &[u8], modulus: usize) -> usize {
    let mut digest = Sha256::new();
    digest.input_str(&format!("{id}{nonce}"));

    // hash the key and get the result
    digest.input(data);
    let mut hash: Vec<u8> = vec![0; digest.output_bytes()];
    digest.result(&mut hash);

    // convert hash into a big integer and perform modulo k
    let int_value = BigUint::from_bytes_le(&hash);
    int_value
        .mod_floor(&BigUint::from(modulus))
        .to_usize()
        .unwrap()
}

pub mod choices;
pub mod cuckoo;
pub mod pung;
pub mod replication;
pub mod sharding;

#[cfg(test)]
mod test;
