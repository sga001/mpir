#![feature(test)]
#![allow(non_snake_case)]
#![feature(custom_attribute, custom_derive, plugin)]

extern crate criterion;
extern crate rand;
extern crate test;
extern crate mpir;

extern crate serde;

#[macro_use]
extern crate serde_derive;

use criterion::Bencher;
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
    ($name:ident, $k:expr, $num:expr, $logt:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);
   
            fn $name(b: &mut Bencher) {
                let mut rng = ChaChaRng::new_unseeded();

                let mut collection: Vec<Tuple<usize, Element>> = vec![];
                for i in 0..$num as usize {
                    let mut x = [0u8; $size];
                    rng.fill_bytes(&mut x);
                    collection.push(Tuple { t: (i, Element { e: x }) });
                }


                let code = CuckooCode::new($k, 3, 1.3); 

                b.iter(|| {
                        let buckets = code.encode(&collection);
                        MultiPirServer::new_setup(&buckets[..], ($size + get_size!(usize)) as u32, 
                                                  2048, $logt,  $d);
                    }
                );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}


macro_rules! pir_setup_pung {
    ($name:ident, $k:expr, $num:expr, $logt:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            fn $name(b: &mut Bencher) {
                let mut rng = ChaChaRng::new_unseeded();

                let mut collection: Vec<Tuple<usize, Element>> = vec![];

                for i in 0..$num as usize {
                    let mut x = [0u8; $size];
                    rng.fill_bytes(&mut x);
                    collection.push(Tuple { t: (i, Element { e: x } ) });
                }

                let code = PungCode::new($k);

                b.iter(|| {
                        let buckets = code.encode(&collection);
                        MultiPirServer::new_setup(&buckets[..], ($size + get_size!(usize)) as u32, 
                                                  2048, $logt, $d);
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
        for i in 0..$num as usize {
            let mut x = [0u8; $size];
            $rng.fill_bytes(&mut x);
            collection.push(Tuple { t: (i, Element { e: x } ) });
        }

        let oracle = $code.encode(&collection);
        let sizes: Vec<(u32, u32)> = oracle.iter()
            .map(|vec| (vec.len() as u32, get_size!((usize, Element)) as u32))
            .collect();

        (oracle, sizes)
    }}
}


macro_rules! pir_query_cuckoo {
    ($name:ident, $k:expr, $num:expr, $logt:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let code = CuckooCode::new($k, 3, 1.3);

                let (oracle, sizes) = generate_oracle!($size, $num, code, rng); 

                // Create the client
                let client = MultiPirClient::new(&sizes, 2048, $logt, $d);

                // Generate keys (desired indexes)
                let mut key_set: HashSet<usize> = HashSet::new();
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $num as usize);
                }

                let keys: Vec<usize> = key_set.drain().collect();

                b.iter(move || {
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
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}


macro_rules! pir_query_pung {
    ($name:ident, $k:expr, $num:expr, $logt:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let mut code = PungCode::new($k);

                let (oracle, sizes) = generate_oracle!($size, $num, code, rng); 

                // Create the client
                let client = MultiPirClient::new(&sizes, 2048, $logt, $d);

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
                    key_set.insert(rng.next_u32() as usize % $num as usize);
                }

                let keys: Vec<usize> = key_set.drain().collect();

                b.iter(move || {

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
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}

macro_rules! pir_reply_cuckoo {
    ($name:ident, $k:expr, $num:expr, $logt:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let code = CuckooCode::new($k, 3, 1.3);

                let (oracle, sizes) = generate_oracle!($size, $num, code, rng); 

                // Generate keys (desired indexes)
                let mut key_set: HashSet<usize> = HashSet::new();
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $num as usize);
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
                let client = MultiPirClient::new(&sizes, 2048, $logt, $d);
                let mut server = MultiPirServer::new(&sizes, 2048, $logt,  $d);
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

                b.iter(|| server.gen_replies(&query, 0) );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}

macro_rules! pir_reply_pung {
    ($name:ident, $k:expr, $num:expr, $logt:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let mut code = PungCode::new($k);

                let (oracle, sizes) = generate_oracle!($size, $num, code, rng); 

                // Create the client and the server
                let client = MultiPirClient::new(&sizes, 2048, $logt, $d);
                let mut server = MultiPirServer::new(&sizes, 2048, $logt, $d);
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
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $num as usize);
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

                b.iter(|| server.gen_replies(&query, 0) );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}

macro_rules! pir_decode_cuckoo {
    ($name:ident, $k:expr, $num:expr, $logt:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            {
                // Measure network
                println!("----------------CUCKOO SIZE RESULT------------------\n");
                let mut rng = ChaChaRng::new_unseeded();
                let code = CuckooCode::new($k, 3, 1.3);

                let (oracle, sizes) = generate_oracle!($size, $num, code, rng); 

                // Generate keys (desired indexes)
                let mut key_set: HashSet<usize> = HashSet::new();
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $num as usize);
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
                let client = MultiPirClient::new(&sizes, 2048, $logt, $d);
                let mut server = MultiPirServer::new(&sizes, 2048, $logt,  $d);
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

                println!("{} query size: {} bytes", stringify!($name), query_size);
                println!("{} reply size: {} bytes", stringify!($name), reply_size);

                println!("-----------------------------------------------------\n");
            }

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let code = CuckooCode::new($k, 3, 1.3);

                let (oracle, sizes) = generate_oracle!($size, $num, code, rng); 

                // Generate keys (desired indexes)
                let mut key_set: HashSet<usize> = HashSet::new();
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $num as usize);
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
                let client = MultiPirClient::new(&sizes, 2048, $logt, $d);
                let mut server = MultiPirServer::new(&sizes, 2048, $logt,  $d);
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

                b.iter(|| client.decode_replies::<Tuple<usize, Element>>(&ind_vec[..], &reply) );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}

macro_rules! pir_decode_pung {
    ($name:ident, $k:expr, $num:expr, $logt:expr, $d:expr, $size:expr) => (

        #[test]
        fn $name() {
            element!($size);

            {
                // Measure network
                println!("----------------PUNG SIZE RESULT------------------\n");
                let mut rng = ChaChaRng::new_unseeded();
                let mut code = PungCode::new($k);

                let (oracle, sizes) = generate_oracle!($size, $num, code, rng); 

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
                    key_set.insert(rng.next_u32() as usize % $num as usize);
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
                let client = MultiPirClient::new(&sizes, 2048, $logt, $d);
                let mut server = MultiPirServer::new(&sizes, 2048, $logt,  $d);
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

                println!("{} query size: {} bytes", stringify!($name), query_size);
                println!("{} reply size: {} bytes", stringify!($name), reply_size);

                println!("-----------------------------------------------------\n");
            }

            // Measure time
            fn $name(b: &mut Bencher) {

                let mut rng = ChaChaRng::new_unseeded();
                let mut code = PungCode::new($k);

                let (oracle, sizes) = generate_oracle!($size, $num, code, rng); 

                // Create the client and the server
                let client = MultiPirClient::new(&sizes, 2048, $logt, $d);
                let mut server = MultiPirServer::new(&sizes, 2048, $logt,  $d);
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
                while key_set.len() < $k { 
                    key_set.insert(rng.next_u32() as usize % $num as usize);
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
                b.iter(||  client.decode_replies::<Tuple<usize, Element>>(&ind_vec[..], &reply) );
            }

            let mut bmark = bmark_settings!();
            bmark.bench_function(stringify!($name), $name);
        }
    )
}


// Parameters: 
// bench name, k, number of entries, logt, d, size of each entry

const SIZE: u32 = 1 << 16;
const ELE_SIZE: usize = 288 - 8; // usize since index adds 4 bytes

// SETUP
pir_setup_cuckoo!(setup_cuckoo_k16, 16, SIZE, 20, 2, ELE_SIZE);
pir_setup_cuckoo!(setup_cuckoo_k64, 64, SIZE, 20, 2, ELE_SIZE);
pir_setup_cuckoo!(setup_cuckoo_k256, 256, SIZE, 20, 2, ELE_SIZE);

pir_setup_pung!(setup_pung_k16, 16, SIZE, 20, 2, ELE_SIZE);
pir_setup_pung!(setup_pung_k64, 64, SIZE, 20, 2, ELE_SIZE);
pir_setup_pung!(setup_pung_k256, 256, SIZE, 20, 2, ELE_SIZE);

// QUERY
pir_query_cuckoo!(query_cuckoo_k16, 16, SIZE, 20, 2, ELE_SIZE);
pir_query_cuckoo!(query_cuckoo_k64, 64, SIZE, 20, 2, ELE_SIZE);
pir_query_cuckoo!(query_cuckoo_k256, 256, SIZE, 20, 2, ELE_SIZE);

pir_query_pung!(query_pung_k16, 16, SIZE, 20, 2, ELE_SIZE);
pir_query_pung!(query_pung_k64, 64, SIZE, 20, 2, ELE_SIZE);
pir_query_pung!(query_pung_k256, 256, SIZE, 20, 2, ELE_SIZE);

// REPLY 
pir_reply_cuckoo!(reply_cuckoo_k16, 16, SIZE, 20, 2, ELE_SIZE);
pir_reply_cuckoo!(reply_cuckoo_k64, 64, SIZE, 20, 2, ELE_SIZE);
pir_reply_cuckoo!(reply_cuckoo_k256, 256, SIZE, 20, 2, ELE_SIZE);

pir_reply_pung!(reply_pung_k16, 16, SIZE, 20, 2, ELE_SIZE);
pir_reply_pung!(reply_pung_k64, 64, SIZE, 20, 2, ELE_SIZE);
pir_reply_pung!(reply_pung_k256, 256, SIZE, 20, 2, ELE_SIZE);

// DECODE
pir_decode_cuckoo!(decode_cuckoo_k16, 16, SIZE, 20, 2, ELE_SIZE);
pir_decode_cuckoo!(decode_cuckoo_k64, 64, SIZE, 20, 2, ELE_SIZE);
pir_decode_cuckoo!(decode_cuckoo_k256, 256, SIZE, 20, 2, ELE_SIZE);

pir_decode_pung!(decode_pung_k16, 16, SIZE, 20, 2, ELE_SIZE);
pir_decode_pung!(decode_pung_k64, 64, SIZE, 20, 2, ELE_SIZE);
pir_decode_pung!(decode_pung_k256, 256, SIZE, 20, 2, ELE_SIZE);
