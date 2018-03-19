extern crate multipir;
extern crate rand;

use std::mem;
use mpir::client::MultiPirClient;
use mpir::server::MultiPirServer;
use mpir::pbc::{BatchCode, Tuple};
use mpir::pbc::replication::ReplicationCode;
use mpir::pbc::sharding::ShardingCode;
use mpir::pbc::choices::ChoicesCode;
use mpir::pbc::cuckoo::CuckooCode;
use mpir::pbc::pung::PungCode;
use rand::Rng;
use std::collections::HashMap;

macro_rules! get_size {
    ($d_type:ty) => (mem::size_of::<$d_type>());
}


fn multipir_test<T>(k: usize, code: &T)
where
    T: BatchCode<usize, usize>,
{
    let mut rng = rand::thread_rng();
    let collection: Vec<Tuple<usize, usize>> = (0..500).map(|e| Tuple { t: (e, e * e) }).collect();

    let encoded_collection = code.encode(&collection);
    let truth = encoded_collection.clone();
    let num_collections = encoded_collection.len();

    let sizes: Vec<u64> = vec![get_size!(Tuple<usize, usize>) as u64; encoded_collection.len()];

    let num_elems: Vec<u64> = encoded_collection
        .iter()
        .map(|vec| vec.len() as u64)
        .collect();

    // Create the client and server
    let client = MultiPirClient::new(&sizes, &num_elems);
    let server = MultiPirServer::new(&encoded_collection);

    // Generate queries
    let keys: Vec<usize> = (0..k).collect();

    // Get schedule
    let schedule: HashMap<usize, Vec<usize>> = code.get_schedule(&keys).unwrap();

    let indexes: HashMap<usize, usize> = schedule
        .iter()
        .map(|(key, buckets)| (key, buckets[0]))
        .map(|(key, bucket)| {
            (
                *key,
                truth[bucket].iter().position(|e| e.t.0 == *key).unwrap(),
            )
        })
        .collect();

    let mut ind_vec = Vec::with_capacity(num_collections);

    for bucket in 0..num_collections {
        if indexes.contains_key(&bucket) {
            ind_vec.push(indexes[&bucket] as u64);
        } else {
            ind_vec.push(rng.next_u32() as u64 % num_elems[bucket]);
        }
    }

    let queries = client.gen_query(&ind_vec);
    let answers = server.gen_answers(&queries);
    let results = client.decode_answers::<Tuple<usize, usize>>(&answers);

    for (bucket, result) in results.iter().enumerate() {
        if indexes.contains_key(&bucket) {
            assert_eq!(result.result, truth[bucket][indexes[&bucket]]);
        }
    }

    // TODO: check to make sure they decode propoerly
}


#[test]
fn multipir_test_replication() {
    let k = 8;
    let code = ReplicationCode::new(k);
    multipir_test(k, &code);
}

#[test]
fn multipir_test_sharding() {
    let k = 16;
    let code = ShardingCode::new(k);
    multipir_test(k, &code);
}

#[test]
fn multipir_test_choice() {
    let k = 16;
    let choices = 2;
    let code = ChoicesCode::new(k, choices);
    multipir_test(k, &code);
}

#[test]
fn multipir_test_cuckoo() {
    let k = 16;
    let replica = 3;
    let factor = 1.3;
    let code = CuckooCode::new(k, replica, factor);
    multipir_test(k, &code);
}


#[test]
fn multipir_test_pung() {
    let k = 8;
    let mut code = PungCode::new(k);

    let mut rng = rand::thread_rng();
    let collection: Vec<Tuple<usize, usize>> = (0..5000).map(|e| Tuple { t: (e, e * e) }).collect();

    let encoded_collection = code.encode(&collection);
    let truth = encoded_collection.clone();
    let num_collections = encoded_collection.len();

    // Get label mapping
    let mut labels: HashMap<usize, Vec<usize>> = HashMap::new();

    for (i, vec_tuple) in encoded_collection.iter().enumerate() {
        for tuple in vec_tuple {
            let entry = labels.entry(tuple.t.0).or_insert(Vec::new());
            entry.push(i);
        }
    }

    code.set_labels(labels);

    let sizes: Vec<u64> = vec![get_size!(Tuple<usize, usize>) as u64; encoded_collection.len()];

    let num_elems: Vec<u64> = encoded_collection
        .iter()
        .map(|vec| vec.len() as u64)
        .collect();

    // Create the client and server
    let client = MultiPirClient::new(&sizes, &num_elems);
    let server = MultiPirServer::new(&encoded_collection);

    // Generate queries
    let keys: Vec<usize> = (0..k).collect();

    // Get schedule
    // Schedule maps from: key to [buckets]
    let schedule: HashMap<usize, Vec<usize>> = (&code as &BatchCode<usize, usize>)
        .get_schedule(&keys)
        .unwrap();

    // Indexes map from bucket -> index to fetch in that bucket
    let mut indexes = HashMap::new();

    for (key, buckets) in schedule {
        // find the index within the sub-buckets for this key.
        let sub_bucket_i = (buckets[0] / 9) * 9;

        let mut index = 0;
        let mut found = false;

        for i in 0..4 {
            // buckets that have keys
            let pos = truth[sub_bucket_i + i].iter().position(|e| e.t.0 == key);

            if pos.is_some() {
                index = pos.unwrap();
                found = true;
                break;
            }
        }

        if !found {
            panic!("Index for key not found");
        }

        for i in 0..buckets.len() {
            indexes.insert(buckets[i], index);
        }
    }

    let mut ind_vec = Vec::with_capacity(num_collections);

    for bucket in 0..num_collections {
        if indexes.contains_key(&bucket) {
            ind_vec.push(indexes[&bucket] as u64);
        } else {
            ind_vec.push(rng.next_u32() as u64 % num_elems[bucket]);
        }
    }

    let queries = client.gen_query(&ind_vec);
    let answers = server.gen_answers(&queries);
    let results = client.decode_answers::<Tuple<usize, usize>>(&answers);

    for (bucket, result) in results.iter().enumerate() {
        if indexes.contains_key(&bucket) {
            assert_eq!(result.result, truth[bucket][indexes[&bucket]]);
        }
    }

    // TODO: check to make sure they decode propoerly
}
