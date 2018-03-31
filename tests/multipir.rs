#![feature(custom_attribute, custom_derive, plugin)]

extern crate mpir;
extern crate rand;
extern crate serde;

#[macro_use]
extern crate serde_derive;

use std::collections::HashSet;
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

const SIZE: usize = 288 - 8; // the index (acting as key) takes up the other 8 bytes
const DIM: u32 = 2;
const LOGT: u32 = 20;
const POLY_DEGREE: u32 = 2048;
const NUM: u32 = 1 << 20;
const BATCH: [usize; 3] = [16, 64, 256];

#[derive(Serialize, Clone)]
struct Element {
    #[serde(serialize_with="<[_]>::serialize")]
    e: [u8; SIZE], 
}

impl std::ops::BitXor for Element {
    type Output = Self;

    fn bitxor(self, rhs: Self) -> Self {
   
        let mut el = Element { e: [0u8; SIZE] };

        for i in 0..self.e.len() {
            el.e[i] = self.e[i] ^ rhs.e[i];
        }

        el
    }
}

impl std::ops::BitXorAssign for Element {

    fn bitxor_assign(&mut self, rhs: Self) {
        for i in 0..self.e.len() {
            self.e[i] ^= rhs.e[i];
        }
    }
}

fn get_oracle(code: &BatchCode<usize, Element>, rng: &mut Rng) 
    -> (Vec<Vec<Tuple<usize, Element>>>, Vec<(u32, u32)>) {

    let mut collection = vec![];

    // we do this to construct the Oracle
    for i in 0..NUM as usize {
        let mut x = [0u8; SIZE];
        rng.fill_bytes(&mut x);
        collection.push(Tuple { t: (i, Element { e: x } ) });
    }

    let oracle = code.encode(&collection);
    let sizes: Vec<(u32, u32)> = oracle.iter()
        .map(|vec| (vec.len() as u32, mem::size_of::<(usize, Element)>() as u32))
        .collect();

    (oracle, sizes)
}

#[test]
fn mpir_sizes() {
    let mut rng = rand::thread_rng();
    
   
    for k in &BATCH {
   

        let code = CuckooCode::new(*k, 3, 1.5);
        let (oracle, sizes) = get_oracle(&code, &mut rng); 

        // Generate keys (desired indexes)
        let mut key_set: HashSet<usize> = HashSet::new();
        while key_set.len() < *k { 
            key_set.insert(rng.next_u32() as usize % NUM as usize);
        }

        let keys: Vec<usize> = key_set.drain().collect();

        // Get schedule. Hash in the head. Simulate entries.
        let schedule: HashMap<usize, Vec<usize>> = (&code as &BatchCode<usize, Element>)
            .get_schedule(&keys)
            .unwrap();

        // Consult the oracle for the indices
        let indexes: HashMap<usize, usize> = schedule
            .iter()
            .map(|(key, buckets)| (key, buckets[0]))
            .map(|(key, bucket)| {
                (
                    *key,
                    oracle[bucket]
                        .iter()
                        .position(|e| e.t.0 == *key)
                        .unwrap(),
                )
            })
            .collect();

        // Create the client and the server
        let client = MultiPirClient::new(&sizes, POLY_DEGREE, LOGT, DIM);
        let mut server = MultiPirServer::new(&sizes, POLY_DEGREE, LOGT, DIM);
        server.setup(&oracle);

        let galois = client.get_galois_keys();
        server.set_galois_keys(&galois, 0);

        let mut ind_vec = Vec::with_capacity(oracle.len());

        for bucket in 0..oracle.len() {
            if indexes.contains_key(&bucket) {
                ind_vec.push(indexes[&bucket] as u32);
            } else {
                ind_vec.push(rng.next_u32() % sizes[bucket].0);
            }
        }

        let query = client.gen_query(&ind_vec);
        let reply = server.gen_replies(&query, 0);

        let mut query_size = 0;
        let mut reply_size = 0;

        for q in &query {
            query_size += q.query.len();
        }

        for r in &reply {
            reply_size += r.reply.len();
        }

        println!("cuckoo query: num {}, k {}, size {}, size/k {}", 
                 NUM, *k, query_size / 1024, query_size / (*k * 1024));
        println!("cuckoo reply num {}, k {}, size {}, size/k {}", 
                 NUM, *k, reply_size / 1024, reply_size / (*k * 1024));
    }


    for k in &BATCH {
        // setup
        let mut rng = rand::thread_rng();
        let mut code = PungCode::new(*k);

        let (oracle, sizes) = get_oracle(&code, &mut rng); 

        // Create the client and the server
        let client = MultiPirClient::new(&sizes, POLY_DEGREE, LOGT, DIM);
        let mut server = MultiPirServer::new(&sizes, POLY_DEGREE, LOGT, DIM);
        server.setup(&oracle);

        let galois = client.get_galois_keys();
        server.set_galois_keys(&galois, 0);


        // Get label mapping (since Pung is data-dependent...)
        let mut labels: HashMap<usize, Vec<usize>> = HashMap::new();

        for (i, vec_tuple) in oracle.iter().enumerate() {
            for tuple in vec_tuple {
                let entry = labels.entry(tuple.t.0).or_insert_with(Vec::new);
                entry.push(i);
            }
        }

        // Pung's hybrid is a data-dependent code
        code.set_labels(labels);

        // Generate keys (desired indexes)
        let mut key_set: HashSet<usize> = HashSet::new();
        while key_set.len() < *k { 
            key_set.insert(rng.next_u32() as usize % NUM as usize);
        }

        let keys: Vec<usize> = key_set.drain().collect();

        // Get schedule. Hash in the head. Simulate entries.
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
                // Consult the oracle for the indices
                // buckets that have keys
                let pos = oracle[sub_bucket_i + i]
                    .iter()
                    .position(|e| e.t.0 == key);

                if pos.is_some() {
                    index = pos.unwrap();
                    found = true;
                    break;
                }
            }

            if !found {
                panic!("Index for key not found");
            }

            for bucket in buckets {
                indexes.insert(bucket, index);
            }
        }

        let mut ind_vec = Vec::with_capacity(oracle.len());

        for bucket in 0..oracle.len() {
            if indexes.contains_key(&bucket) {
                ind_vec.push(indexes[&bucket] as u32);
            } else {
                ind_vec.push(rng.next_u32() % sizes[bucket].0);
            }
        }

        let query = client.gen_query(&ind_vec);
        let reply = server.gen_replies(&query, 0);

        let mut query_size = 0;
        let mut reply_size = 0;

        for q in &query {
            query_size += q.query.len();
        }

        for r in &reply {
            reply_size += r.reply.len();
        }

        println!("pung query: num {}, k {}, size {}, size/k {}", 
                 NUM, *k, query_size / 1024, query_size / (*k * 1024));
        println!("pung reply num {}, k {}, size {}, size/k {}", 
                 NUM, *k, reply_size / 1024, reply_size / (*k * 1024));

    }
}





fn multipir_test<T>(k: usize, code: &T)
where
    T: BatchCode<usize, Element>,
{
    let mut rng = rand::thread_rng();
    let (oracle, sizes) = get_oracle(code, &mut rng); 

    let truth = oracle.clone();

    // Generate keys (desired indexes)
    let mut key_set: HashSet<usize> = HashSet::new();
    while key_set.len() < k { 
        key_set.insert(rng.next_u32() as usize % NUM as usize);
    }

    let keys: Vec<usize> = key_set.drain().collect();

    // Get schedule. Hash in the head. Simulate entries.
    let schedule: HashMap<usize, Vec<usize>> = (code as &BatchCode<usize, Element>)
        .get_schedule(&keys)
        .unwrap();

    // Consult the oracle for the indices
    let indexes: HashMap<usize, usize> = schedule
        .iter()
        .map(|(key, buckets)| (key, buckets[0]))
        .map(|(key, bucket)| {
            (
                *key,
                oracle[bucket]
                    .iter()
                    .position(|e| e.t.0 == *key)
                    .unwrap(),
            )
        })
        .collect();

    // Create the client and the server
    let client = MultiPirClient::new(&sizes, POLY_DEGREE, LOGT, DIM);
    let mut server = MultiPirServer::new(&sizes, POLY_DEGREE, LOGT, DIM);
    server.setup(&oracle);

    let galois = client.get_galois_keys();
    server.set_galois_keys(&galois, 0);

    let mut ind_vec = Vec::with_capacity(oracle.len());

    for bucket in 0..oracle.len() {
        if indexes.contains_key(&bucket) {
            ind_vec.push(indexes[&bucket] as u32);
        } else {
            ind_vec.push(rng.next_u32() % sizes[bucket].0);
        }
    }

    let query = client.gen_query(&ind_vec);
    let reply = server.gen_replies(&query, 0);
    let results = client.decode_replies::<Tuple<usize, Element>>(&ind_vec[..], &reply);

    for (bucket, result) in results.iter().enumerate() {
        if indexes.contains_key(&bucket) {
            assert!(result.t.0 == truth[bucket][indexes[&bucket]].t.0);
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
    let factor = 1.5;
    let code = CuckooCode::new(k, replica, factor);
    multipir_test(k, &code);
}


#[test]
fn multipir_test_pung() {

    let k = 16;

    // setup
    let mut rng = rand::thread_rng();
    let mut code = PungCode::new(k);

    let (oracle, sizes) = get_oracle(&code, &mut rng); 
    let truth = oracle.clone();

    // Create the client and the server
    let client = MultiPirClient::new(&sizes, POLY_DEGREE, LOGT, DIM);
    let mut server = MultiPirServer::new(&sizes, POLY_DEGREE, LOGT, DIM);
    server.setup(&oracle);

    let galois = client.get_galois_keys();
    server.set_galois_keys(&galois, 0);


    // Get label mapping (since Pung is data-dependent...)
    let mut labels: HashMap<usize, Vec<usize>> = HashMap::new();

    for (i, vec_tuple) in oracle.iter().enumerate() {
        for tuple in vec_tuple {
            let entry = labels.entry(tuple.t.0).or_insert_with(Vec::new);
            entry.push(i);
        }
    }

    // Pung's hybrid is a data-dependent code
    code.set_labels(labels);

    // Generate keys (desired indexes)
    let mut key_set: HashSet<usize> = HashSet::new();
    while key_set.len() < k { 
        key_set.insert(rng.next_u32() as usize % NUM as usize);
    }

    let keys: Vec<usize> = key_set.drain().collect();

    // Get schedule. Hash in the head. Simulate entries.
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
            // Consult the oracle for the indices
            // buckets that have keys
            let pos = oracle[sub_bucket_i + i]
                .iter()
                .position(|e| e.t.0 == key);

            if pos.is_some() {
                index = pos.unwrap();
                found = true;
                break;
            }
        }

        if !found {
            panic!("Index for key not found");
        }

        for bucket in buckets {
            indexes.insert(bucket, index);
        }
    }

    let mut ind_vec = Vec::with_capacity(oracle.len());

    for bucket in 0..oracle.len() {
        if indexes.contains_key(&bucket) {
            ind_vec.push(indexes[&bucket] as u32);
        } else {
            ind_vec.push(rng.next_u32() % sizes[bucket].0);
        }
    }

    let query = client.gen_query(&ind_vec);
    let reply = server.gen_replies(&query, 0);
    let results = client.decode_replies::<Tuple<usize, Element>>(&ind_vec[..], &reply);

    for (bucket, result) in results.iter().enumerate() {
        if indexes.contains_key(&bucket) {
            assert!(result.t.0 == truth[bucket][indexes[&bucket]].t.0);
        }
    }
}


