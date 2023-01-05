#![feature(test)]

extern crate mpir;
extern crate rand;
extern crate test;

use mpir::pbc::choices::ChoicesCode;
use mpir::pbc::cuckoo::CuckooCode;
use mpir::pbc::pung::PungCode;
use mpir::pbc::replication::ReplicationCode;
use mpir::pbc::sharding::ShardingCode;
use mpir::pbc::{BatchCode, Tuple};
use rand::Rng;
use std::collections::HashMap;
use std::collections::HashSet;
use test::Bencher;

const K: usize = 64;
const SIZE: usize = 32768;

fn get_schedule<T>(b: &mut Bencher, code: &T)
where
    T: BatchCode<usize, usize>,
{
    let mut rng = rand::thread_rng();

    let mut key_set: HashSet<usize> = HashSet::new();
    while key_set.len() < K {
        key_set.insert(rng.next_u32() as usize % SIZE);
    }
    let keys: Vec<usize> = key_set.drain().collect();

    b.iter(|| {
        code.get_schedule(&keys).unwrap();
    });
}

fn encode<T>(b: &mut Bencher, code: &T)
where
    T: BatchCode<usize, usize>,
{
    let collection: Vec<Tuple<usize, usize>> = (0..SIZE).map(|e| Tuple { t: (e, e * e) }).collect();

    b.iter(|| code.encode(&collection));
}

fn decode<T>(b: &mut Bencher, code: &T)
where
    T: BatchCode<usize, usize>,
{
    let mut rng = rand::thread_rng();
    let collection: Vec<Tuple<usize, usize>> = (0..SIZE).map(|e| Tuple { t: (e, e * e) }).collect();
    let oracle = code.encode(&collection);
    let num_elems: Vec<u64> = oracle.iter().map(|vec| vec.len() as u64).collect();

    let mut key_set: HashSet<usize> = HashSet::new();
    while key_set.len() < K {
        key_set.insert(rng.next_u32() as usize % SIZE);
    }

    let keys: Vec<usize> = key_set.drain().collect();
    let schedule = code.get_schedule(&keys).expect("Schedule failed");

    let indexes: HashMap<usize, usize> = schedule
        .iter()
        .map(|(key, buckets)| (key, buckets[0]))
        .map(|(key, bucket)| {
            (
                *key,
                oracle[bucket].iter().position(|e| e.t.0 == *key).unwrap(),
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

    b.iter(|| {
        for bucket in 0..oracle.len() {
            let codewords = vec![oracle[bucket][ind_vec[bucket] as usize].clone()];
            code.decode(&codewords);
        }
    });
}

#[bench]
fn bench_schedule_replication(b: &mut Bencher) {
    let code = ReplicationCode::new(K);
    get_schedule(b, &code);
}

#[bench]
fn bench_schedule_sharding(b: &mut Bencher) {
    let code = ShardingCode::new(K);
    get_schedule(b, &code);
}

#[bench]
fn bench_schedule_choices(b: &mut Bencher) {
    let code = ChoicesCode::new(K, 2);
    get_schedule(b, &code);
}

#[bench]
fn bench_schedule_cuckoo(b: &mut Bencher) {
    let code = CuckooCode::new(K, 3, 1.3);
    get_schedule(b, &code);
}

#[bench]
fn bench_encode_cuckoo(b: &mut Bencher) {
    let code = CuckooCode::new(K, 3, 1.3);
    encode(b, &code);
}

#[bench]
fn bench_encode_sharding(b: &mut Bencher) {
    let code = ShardingCode::new(K);
    encode(b, &code);
}

#[bench]
fn bench_encode_choices(b: &mut Bencher) {
    let code = ChoicesCode::new(K, 2);
    encode(b, &code);
}

#[bench]
fn bench_encode_pung(b: &mut Bencher) {
    let code = PungCode::new(K);
    encode(b, &code);
}

#[bench]
fn bench_decode_cuckoo(b: &mut Bencher) {
    let code = CuckooCode::new(K, 3, 1.3);
    decode(b, &code);
}

#[bench]
fn bench_decode_sharding(b: &mut Bencher) {
    let code = ShardingCode::new(K);
    decode(b, &code);
}

#[bench]
fn bench_decode_choices(b: &mut Bencher) {
    let code = ChoicesCode::new(K, 2);
    decode(b, &code);
}

#[bench]
fn bench_schedule_pung(b: &mut Bencher) {
    let mut code = PungCode::new(K);
    let mut rng = rand::thread_rng();

    // hack to ensure no duplicates
    let keys: HashSet<usize> = (0..K).map(|_| rng.next_u32() as usize % SIZE).collect();
    let keys: Vec<usize> = keys.iter().cloned().collect();

    // Ughh pung's code is data-dependent so we actually need to create a collection
    let tuples: Vec<Tuple<usize, usize>> = (0..SIZE).map(|e| Tuple { t: (e, e * e) }).collect();
    let db: Vec<Vec<Tuple<usize, usize>>> = code.encode(&tuples);

    // Get label mapping
    let mut labels: HashMap<usize, Vec<usize>> = HashMap::new();

    for (i, vec_tuple) in db.iter().enumerate() {
        for tuple in vec_tuple {
            let entry = labels.entry(tuple.t.0).or_insert_with(Vec::new);
            entry.push(i);
        }
    }

    code.set_labels(labels);

    b.iter(|| {
        (&code as &dyn BatchCode<usize, usize>)
            .get_schedule(&keys)
            .unwrap();
    });
}
