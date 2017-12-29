#![feature(test)]
#![allow(non_snake_case)]
#![feature(custom_attribute, custom_derive, plugin)]

extern crate criterion;
extern crate rand;
extern crate test;
extern crate multipir;

extern crate serde;

#[macro_use]
extern crate serde_derive;

use criterion::Bencher;
use std::time::Duration;
use multipir::client::MultiPirClient;
use multipir::server::MultiPirServer;
use multipir::pbc::{BatchCode, Tuple};
use multipir::pbc::cuckoo::CuckooCode;
use multipir::pbc::pung::PungCode;

use rand::ChaChaRng;
use rand::Rng;
use std::collections::HashMap;
use std::collections::HashSet;
use std::mem;

macro_rules! bmark_settings {
    () => {{

        // If you want to change settings call .sample_size() or any of the other options
        // Example:
        let mut crit = criterion::Criterion::default();
        crit.sample_size(10)
            .measurement_time(Duration::new(0, 50000000)); // in (sec, ns)

        crit
    }};
}


macro_rules! get_size {
    ($d_type:ty) => (mem::size_of::<$d_type>());
}

macro_rules! element {
    ($size:expr) => {
        #[derive(Serialize, Clone)]
        struct Element {
            #[serde(serialize_with="<[_]>::serialize")]
            e: [u8; $size], 
        }

        impl std::ops::BitXor for Element {
            type Output = Self;

            fn bitxor(self, rhs: Self) -> Self {
           
                let mut el = Element { e: [0u8; $size] };

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
    }
}


macro_rules! pir_setup_cuckoo {
    ($name:ident, $k:expr, $num:expr, $alpha:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);
   
            fn $name(b: &mut Bencher) {
                let mut rng = ChaChaRng::new_unseeded();

                let mut collection: Vec<Tuple<usize, Element>> = vec![];
                for i in 0..$num {
                    let mut x = [0u8; $size];
                    rng.fill_bytes(&mut x);
                    collection.push(Tuple { t: (i, Element { e: x }) });
                }

                let code = CuckooCode::new($k, 3, 1.3); 

                b.iter(|| {
                        let buckets = code.encode(&collection);
                        MultiPirServer::with_params(&buckets, $alpha,  $d);
                    }
                );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}


macro_rules! pir_setup_pung {
    ($name:ident, $k:expr, $num:expr, $alpha:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            fn $name(b: &mut Bencher) {
                let mut rng = ChaChaRng::new_unseeded();

                let mut collection = vec![];

                for i in 0..$num {
                    let mut x = [0u8; $size];
                    rng.fill_bytes(&mut x);
                    collection.push(Tuple { t: (i, Element { e: x } ) });
                }

                let code = PungCode::new($k);

                b.iter(|| {
                        let buckets = code.encode(&collection);
                        MultiPirServer::with_params(&buckets, $alpha,  $d);
                    }
                );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}


macro_rules! generate_oracle {
    ($size:expr, $num:expr, $code:expr, $rng:expr) => {{


        let mut collection = vec![];

        // we do this to construct the Oracle
        for i in 0..$num {
            let mut x = [0u8; $size];
            $rng.fill_bytes(&mut x);
            collection.push(Tuple { t: (i, Element { e: x } ) });
        }

        let oracle = $code.encode(&collection);
        let sizes: Vec<u64> = vec![get_size!((usize, Element)) as u64; oracle.len()];
        let num_elems: Vec<u64> = oracle.iter().map(|vec| vec.len() as u64).collect();

        (oracle, sizes, num_elems)
    }}
}


macro_rules! pir_query_cuckoo {
    ($name:ident, $k:expr, $num:expr, $alpha:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let code = CuckooCode::new($k, 3, 1.3);

                let (oracle, sizes, num_elems) = generate_oracle!($size, $num, code, rng); 

                // Create the client
                let client = MultiPirClient::with_params(&sizes, &num_elems, $alpha, $d);

                b.iter_with_setup(move || {
                        // Generate keys (desired indexes)
                        let mut key_set: HashSet<usize> = HashSet::new();
                        while key_set.len() < $k { 
                            key_set.insert(rng.next_u32() as usize % $size);
                        }

                        let keys: Vec<usize> = key_set.drain().collect();
                        keys
                    }, move |keys| {

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
                            ind_vec.push(indexes[&bucket] as u64);
                        } else {
                            ind_vec.push(u64::from(rng.next_u32()) % num_elems[bucket]);
                        }
                    }

                    client.gen_query(&ind_vec);
              });
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}


macro_rules! pir_query_pung {
    ($name:ident, $k:expr, $num:expr, $alpha:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let mut code = PungCode::new($k);

                let (oracle, sizes, num_elems) = generate_oracle!($size, $num, code, rng); 

                // Create the client
                let client = MultiPirClient::with_params(&sizes, &num_elems, $alpha, $d);

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

                b.iter_with_setup(move || {
                        // Generate keys (desired indexes)
                        let mut key_set: HashSet<usize> = HashSet::new();
                        while key_set.len() < $k { 
                            key_set.insert(rng.next_u32() as usize % $size);
                        }

                        let keys: Vec<usize> = key_set.drain().collect();
                        keys
                    }, move |keys| {


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
                            ind_vec.push(indexes[&bucket] as u64);
                        } else {
                            ind_vec.push(u64::from(rng.next_u32()) % num_elems[bucket]);
                        }
                    }

                    client.gen_query(&ind_vec);
              });
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}

macro_rules! pir_answer_cuckoo {
    ($name:ident, $k:expr, $num:expr, $alpha:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let code = CuckooCode::new($k, 3, 1.3);

                let (oracle, sizes, num_elems) = generate_oracle!($size, $num, code, rng); 

                // Generate keys (desired indexes)
                let mut key_set: HashSet<usize> = HashSet::new();
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $size);
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
                let client = MultiPirClient::with_params(&sizes, &num_elems, $alpha, $d);
                let server = MultiPirServer::with_params(&oracle, $alpha,  $d);

                let mut ind_vec = Vec::with_capacity(oracle.len());

                for bucket in 0..oracle.len() {
                    if indexes.contains_key(&bucket) {
                        ind_vec.push(indexes[&bucket] as u64);
                    } else {
                        ind_vec.push(u64::from(rng.next_u32()) % num_elems[bucket]);
                    }
                }


                b.iter_with_setup(move || {
                        client.gen_query(&ind_vec)
                    }, |query| {
                    server.gen_answers(&query);
              });
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}

macro_rules! pir_answer_pung {
    ($name:ident, $k:expr, $num:expr, $alpha:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let mut code = PungCode::new($k);

                let (oracle, sizes, num_elems) = generate_oracle!($size, $num, code, rng); 

                // Create the client and the server
                let client = MultiPirClient::with_params(&sizes, &num_elems, $alpha, $d);
                let server = MultiPirServer::with_params(&oracle, $alpha, $d);

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
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $size);
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
                        ind_vec.push(indexes[&bucket] as u64);
                    } else {
                        ind_vec.push(u64::from(rng.next_u32()) % num_elems[bucket]);
                    }
                }

                b.iter_with_setup(|| {
                        client.gen_query(&ind_vec)
                    }, |query| {
                        server.gen_answers(&query);
                });
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}

macro_rules! pir_decode_cuckoo {
    ($name:ident, $k:expr, $num:expr, $alpha:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            {
                // Measure network
                println!("----------------CUCKOO SIZE RESULT------------------\n");
                let mut rng = ChaChaRng::new_unseeded();
                let code = CuckooCode::new($k, 3, 1.3);

                let (oracle, sizes, num_elems) = generate_oracle!($size, $num, code, rng); 

                // Generate keys (desired indexes)
                let mut key_set: HashSet<usize> = HashSet::new();
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $size);
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
                let client = MultiPirClient::with_params(&sizes, &num_elems, $alpha, $d);
                let server = MultiPirServer::with_params(&oracle, $alpha,  $d);

                let mut ind_vec = Vec::with_capacity(oracle.len());

                for bucket in 0..oracle.len() {
                    if indexes.contains_key(&bucket) {
                        ind_vec.push(indexes[&bucket] as u64);
                    } else {
                        ind_vec.push(u64::from(rng.next_u32()) % num_elems[bucket]);
                    }
                }

                let query = client.gen_query(&ind_vec);
                let answer = server.gen_answers(&query);

                let mut query_size = 0;
                let mut answer_size = 0;

                for q in &query {
                    query_size += q.query.len();    
                }

                for a in &answer {
                    answer_size += a.answer.len();
                }

                println!("{} query size: {} bytes", stringify!($name), query_size);
                println!("{} answer size: {} bytes", stringify!($name), answer_size);

                println!("-----------------------------------------------------\n");
            }

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let code = CuckooCode::new($k, 3, 1.3);

                let (oracle, sizes, num_elems) = generate_oracle!($size, $num, code, rng); 

                // Generate keys (desired indexes)
                let mut key_set: HashSet<usize> = HashSet::new();
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $size);
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
                let client = MultiPirClient::with_params(&sizes, &num_elems, $alpha, $d);
                let server = MultiPirServer::with_params(&oracle, $alpha,  $d);

                let mut ind_vec = Vec::with_capacity(oracle.len());

                for bucket in 0..oracle.len() {
                    if indexes.contains_key(&bucket) {
                        ind_vec.push(indexes[&bucket] as u64);
                    } else {
                        ind_vec.push(u64::from(rng.next_u32()) % num_elems[bucket]);
                    }
                }

                let query = client.gen_query(&ind_vec);
                let answer = server.gen_answers(&query);

                b.iter(|| client.decode_answers::<Tuple<usize, Element>>(&answer) );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}

macro_rules! pir_decode_pung {
    ($name:ident, $k:expr, $num:expr, $alpha:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            {
                // Measure network
                println!("----------------PUNG SIZE RESULT------------------\n");
                let mut rng = ChaChaRng::new_unseeded();
                let mut code = PungCode::new($k);

                let (oracle, sizes, num_elems) = generate_oracle!($size, $num, code, rng); 

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
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $size);
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

                // Create the client and the server
                let client = MultiPirClient::with_params(&sizes, &num_elems, $alpha, $d);
                let server = MultiPirServer::with_params(&oracle, $alpha, $d);

                let mut ind_vec = Vec::with_capacity(oracle.len());

                for bucket in 0..oracle.len() {
                    if indexes.contains_key(&bucket) {
                        ind_vec.push(indexes[&bucket] as u64);
                    } else {
                        ind_vec.push(u64::from(rng.next_u32()) % num_elems[bucket]);
                    }
                }

                let query = client.gen_query(&ind_vec);
                let answer = server.gen_answers(&query);

                let mut query_size = 0;
                let mut answer_size = 0;

                for q in &query {
                    query_size += q.query.len();    
                }

                for a in &answer {
                    answer_size += a.answer.len();
                }

                println!("{} query size: {} bytes", stringify!($name), query_size);
                println!("{} answer size: {} bytes", stringify!($name), answer_size);

                println!("-----------------------------------------------------\n");
            }

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let mut code = PungCode::new($k);

                let (oracle, sizes, num_elems) = generate_oracle!($size, $num, code, rng); 

                // Create the client and the server
                let client = MultiPirClient::with_params(&sizes, &num_elems, $alpha, $d);
                let server = MultiPirServer::with_params(&oracle, $alpha, $d);

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
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $size);
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
                        ind_vec.push(indexes[&bucket] as u64);
                    } else {
                        ind_vec.push(u64::from(rng.next_u32()) % num_elems[bucket]);
                    }
                }

		let query = client.gen_query(&ind_vec);
		let answer = server.gen_answers(&query);

                b.iter(||  client.decode_answers::<Tuple<usize, Element>>(&answer) );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}


// Parameters: 
// bench name, k, number of entries, alpha, d, size of each entry

// SETUP
pir_setup_cuckoo!(bench_multipir_setup_cuckoo_k16_131072, 16, 131072, 32, 2, 288);
pir_setup_cuckoo!(bench_multipir_setup_cuckoo_k64_131072, 64, 131072, 32, 2, 288);
pir_setup_cuckoo!(bench_multipir_setup_cuckoo_k256_131072, 256, 131072, 32, 2, 288);

pir_setup_pung!(bench_multipir_setup_pung_k16_131072, 16, 131072, 32, 2, 288);
pir_setup_pung!(bench_multipir_setup_pung_k64_131072, 64, 131072, 32, 2, 288);
pir_setup_pung!(bench_multipir_setup_pung_k256_131072, 256, 131072, 32, 2, 288);

pir_setup_cuckoo!(bench_multipir_setup_cuckoo_k16_131072_1KB, 16, 131072, 32, 2, 1024);
pir_setup_cuckoo!(bench_multipir_setup_cuckoo_k64_131072_1KB, 64, 131072, 32, 2, 1024);
pir_setup_cuckoo!(bench_multipir_setup_cuckoo_k256_131072_1KB, 256, 131072, 32, 2, 1024);

pir_setup_pung!(bench_multipir_setup_pung_k16_131072_1KB, 16, 131072, 32, 2, 1024);
pir_setup_pung!(bench_multipir_setup_pung_k64_131072_1KB, 64, 131072, 32, 2, 1024);
pir_setup_pung!(bench_multipir_setup_pung_k256_131072_1kb, 256, 131072, 32, 2, 1024);

// QUERY
pir_query_cuckoo!(bench_multipir_query_cuckoo_k16_131072, 16, 131072, 32, 2, 288);
pir_query_cuckoo!(bench_multipir_query_cuckoo_k64_131072, 64, 131072, 32, 2, 288);
pir_query_cuckoo!(bench_multipir_query_cuckoo_k256_131072, 256, 131072, 32, 2, 288);

pir_query_pung!(bench_multipir_query_pung_k16_131072, 16, 131072, 32, 2, 288);
pir_query_pung!(bench_multipir_query_pung_k64_131072, 64, 131072, 32, 2, 288);
pir_query_pung!(bench_multipir_query_pung_k256_131072, 256, 131072, 32, 2, 288);

pir_query_cuckoo!(bench_multipir_query_cuckoo_k16_131072_1KB, 16, 131072, 32, 2, 1024);
pir_query_cuckoo!(bench_multipir_query_cuckoo_k64_131072_1KB, 64, 131072, 32, 2, 1024);
pir_query_cuckoo!(bench_multipir_query_cuckoo_k256_131072_1KB, 256, 131072, 32, 2, 1024);

pir_query_pung!(bench_multipir_query_pung_k16_131072_1KB, 16, 131072, 32, 2, 1024);
pir_query_pung!(bench_multipir_query_pung_k64_131072_1KB, 64, 131072, 32, 2, 1024);
pir_query_pung!(bench_multipir_query_pung_k256_131072_1kb, 256, 131072, 32, 2, 1024);

// ANSWER
/*
pir_answer_cuckoo!(bench_multipir_answer_cuckoo_k16_131072, 16, 131072, 32, 2, 288);
pir_answer_cuckoo!(bench_multipir_answer_cuckoo_k64_131072, 64, 131072, 32, 2, 288);
pir_answer_cuckoo!(bench_multipir_answer_cuckoo_k256_131072, 256, 131072, 32, 2, 288);

pir_answer_pung!(bench_multipir_answer_pung_k16_131072, 16, 131072, 32, 2, 288);
pir_answer_pung!(bench_multipir_answer_pung_k64_131072, 64, 131072, 32, 2, 288);
pir_answer_pung!(bench_multipir_answer_pung_k256_131072, 256, 131072, 32, 2, 288);

pir_answer_cuckoo!(bench_multipir_answer_cuckoo_k16_131072_1KB, 16, 131072, 32, 2, 1024);
pir_answer_cuckoo!(bench_multipir_answer_cuckoo_k64_131072_1KB, 64, 131072, 32, 2, 1024);
pir_answer_cuckoo!(bench_multipir_answer_cuckoo_k256_131072_1KB, 256, 131072, 32, 2, 1024);

pir_answer_pung!(bench_multipir_answer_pung_k16_131072_1KB, 16, 131072, 32, 2, 1024);
pir_answer_pung!(bench_multipir_answer_pung_k64_131072_1KB, 64, 131072, 32, 2, 1024);
pir_answer_pung!(bench_multipir_answer_pung_k256_131072_1kb, 256, 131072, 32, 2, 1024);
*/

// DECODE
pir_decode_cuckoo!(bench_multipir_decode_cuckoo_k16_131072, 16, 131072, 32, 2, 288);
pir_decode_cuckoo!(bench_multipir_decode_cuckoo_k64_131072, 64, 131072, 32, 2, 288);
pir_decode_cuckoo!(bench_multipir_decode_cuckoo_k256_131072, 256, 131072, 32, 2, 288);

pir_decode_pung!(bench_multipir_decode_pung_k16_131072, 16, 131072, 32, 2, 288);
pir_decode_pung!(bench_multipir_decode_pung_k64_131072, 64, 131072, 32, 2, 288);
pir_decode_pung!(bench_multipir_decode_pung_k256_131072, 256, 131072, 32, 2, 288);

pir_decode_cuckoo!(bench_multipir_decode_cuckoo_k16_131072_1KB, 16, 131072, 32, 2, 1024);
pir_decode_cuckoo!(bench_multipir_decode_cuckoo_k64_131072_1KB, 64, 131072, 32, 2, 1024);
pir_decode_cuckoo!(bench_multipir_decode_cuckoo_k256_131072_1KB, 256, 131072, 32, 2, 1024);

pir_decode_pung!(bench_multipir_decode_pung_k16_131072_1KB, 16, 131072, 32, 2, 1024);
pir_decode_pung!(bench_multipir_decode_pung_k64_131072_1KB, 64, 131072, 32, 2, 1024);
pir_decode_pung!(bench_multipir_decode_pung_k256_131072_1kb, 256, 131072, 32, 2, 1024);
