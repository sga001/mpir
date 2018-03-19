#![feature(custom_attribute, custom_derive, plugin)]

#[macro_use]
extern crate criterion;
extern crate rand;
extern crate mpir;
extern crate serde;

#[macro_use]
extern crate serde_derive;

use criterion::Criterion;
use std::time::Duration;
use mpir::client::MultiPirClient;
use mpir::server::MultiPirServer;
use mpir::pbc::{BatchCode, Tuple};
use mpir::pbc::cuckoo::CuckooCode;
use mpir::pbc::pung::PungCode;

use rand::ChaChaRng;
use rand::Rng;
use std::collections::HashMap;
use std::collections::HashSet;
use std::mem;

const SIZE: usize = 288 - 8;
const DIM: u32 = 2;
const LOGT: u32 = 20;
const POLY_DEGREE: u32 = 2048;
const NUM: u32 = 1 << 20;
const BATCH_SIZES: [usize; 3] = [16, 64, 256];

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



fn setup_cuckoo(c: &mut Criterion) {
    c.bench_function_over_inputs(
        &format!("setup_cuckoo_d{}_n{}", DIM, NUM),
        |b, &&k| {
            // setup
            let mut rng = ChaChaRng::new_unseeded();

            let mut collection: Vec<Tuple<usize, Element>> = vec![];
            for i in 0..NUM as usize {
                let mut x = [0u8; SIZE];
                rng.fill_bytes(&mut x);
                collection.push(Tuple { t: (i, Element { e: x }) });
            }

            let code = CuckooCode::new(k, 3, 1.5);

            // measurement
            b.iter(|| {
                let buckets = code.encode(&collection);
                MultiPirServer::new_setup(&buckets[..], (SIZE + mem::size_of::<usize>()) as u32, 
                                          POLY_DEGREE, LOGT, DIM);
            });
        },
        &BATCH_SIZES,
    );
}

fn setup_pung(c: &mut Criterion) {
    c.bench_function_over_inputs(
        &format!("setup_pung_d{}_n{}", DIM, NUM),
        |b, &&k| {
            // setup
            let mut rng = ChaChaRng::new_unseeded();

            let mut collection: Vec<Tuple<usize, Element>> = vec![];
            for i in 0..NUM as usize {
                let mut x = [0u8; SIZE];
                rng.fill_bytes(&mut x);
                collection.push(Tuple { t: (i, Element { e: x }) });
            }

            let code = PungCode::new(k);

            // measurement
            b.iter(|| {
                let buckets = code.encode(&collection);
                MultiPirServer::new_setup(&buckets[..], (SIZE + mem::size_of::<usize>()) as u32, 
                                          POLY_DEGREE, LOGT, DIM);
            });
        },
        &BATCH_SIZES,
    );
}

fn query_cuckoo(c: &mut Criterion) {
    c.bench_function_over_inputs(
        &format!("query_cuckoo_d{}_n{}", DIM, NUM),
        |b, &&k| {
            // setup
            let mut rng = ChaChaRng::new_unseeded();
            let code = CuckooCode::new(k, 3, 1.5);

            let (oracle, sizes) = get_oracle(&code, &mut rng);

            // Create the client
            let client = MultiPirClient::new(&sizes, POLY_DEGREE, LOGT, DIM);

            // Generate keys (desired indexes)
            let mut key_set: HashSet<usize> = HashSet::new();
            while key_set.len() < k { 
                key_set.insert(rng.next_u32() as usize % NUM as usize);
            }

            let keys: Vec<usize> = key_set.drain().collect();

            // measurement
            b.iter(|| {
                // Get schedule
                let schedule: HashMap<usize, Vec<usize>> = (&code as &BatchCode<usize, usize>)
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

                let mut ind_vec = Vec::with_capacity(oracle.len());

                for bucket in 0..oracle.len() {
                    if indexes.contains_key(&bucket) {
                        ind_vec.push(indexes[&bucket] as u32);
                    } else {
                        ind_vec.push(rng.next_u32() % sizes[bucket].0);
                    }
                }

                client.gen_query(&ind_vec);

            });
        },
        &BATCH_SIZES,
    );
}

fn query_pung(c: &mut Criterion) {
    c.bench_function_over_inputs(
        &format!("query_pung_d{}_n{}", DIM, NUM),
        |b, &&k| {
            // setup
            let mut rng = ChaChaRng::new_unseeded();
            let mut code = PungCode::new(k);

            let (oracle, sizes) = get_oracle(&code, &mut rng);

            // Create the client
            let client = MultiPirClient::new(&sizes, POLY_DEGREE, LOGT, DIM);

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

            // measurement
            b.iter(|| {
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

                client.gen_query(&ind_vec);
            });
        },
        &BATCH_SIZES,
    );
}

fn reply_cuckoo(c: &mut Criterion) {
    c.bench_function_over_inputs(
        &format!("reply_cuckoo_d{}_n{}", DIM, NUM),
        |b, &&k| {
            // setup
            let mut rng = ChaChaRng::new_unseeded();
            let code = CuckooCode::new(k, 3, 1.5);

            let (oracle, sizes) = get_oracle(&code, &mut rng); 

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

            // measurement
            b.iter(|| server.gen_replies(&query, 0));
        },
        &BATCH_SIZES,
    );
}

fn reply_pung(c: &mut Criterion) {
    c.bench_function_over_inputs(
        &format!("reply_pung_d{}_n{}", DIM, NUM),
        |b, &&k| {
            // setup
            let mut rng = ChaChaRng::new_unseeded();
            let mut code = PungCode::new(k);

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

            // measurement
            b.iter(|| server.gen_replies(&query, 0));
        },
        &BATCH_SIZES,
    );
}


fn decode_cuckoo(c: &mut Criterion) {
    c.bench_function_over_inputs(
        &format!("decode_cuckoo_d{}_n{}", DIM, NUM),
        |b, &&k| {
            // setup
            let mut rng = ChaChaRng::new_unseeded();
            let code = CuckooCode::new(k, 3, 1.5);

            let (oracle, sizes) = get_oracle(&code, &mut rng); 

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

            // measurement
            b.iter(|| client.decode_replies::<Tuple<usize, Element>>(&ind_vec[..], &reply) );
        },
        &BATCH_SIZES,
    );
}

fn decode_pung(c: &mut Criterion) {
    c.bench_function_over_inputs(
        &format!("decode_pung_d{}_n{}", DIM, NUM),
        |b, &&k| {
            // setup
            let mut rng = ChaChaRng::new_unseeded();
            let mut code = PungCode::new(k);

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

            // measurement
            b.iter(||  client.decode_replies::<Tuple<usize, Element>>(&ind_vec[..], &reply) );
        },
        &BATCH_SIZES,
    );
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::new(5, 0))
        .warm_up_time(Duration::new(1, 0))
        .without_plots();
    targets = setup_cuckoo, setup_pung, query_cuckoo, query_pung, reply_cuckoo, reply_pung,
              decode_cuckoo, decode_pung
}

criterion_main!(benches);
