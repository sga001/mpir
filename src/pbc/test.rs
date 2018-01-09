use rand;
use rand::Rng;
use std::collections::HashMap;

use super::BatchCode;
use super::replication::ReplicationCode;
use super::sharding::ShardingCode;
use super::choices::ChoicesCode;
use super::cuckoo::CuckooCode;
use super::pung::PungCode;
use super::Tuple;

fn do_test(code: &BatchCode<usize, usize>, k: usize, tuples: &[Tuple<usize, usize>]) {
    // server
    let db: Vec<Vec<Tuple<usize, usize>>> = code.encode(&tuples);
    let keys: Vec<usize> = (0..k).collect();

    // client
    let schedule: HashMap<usize, Vec<usize>> = code.get_schedule(&keys).unwrap();

    // verify schedule is valid
    for (key, buckets) in schedule {
        assert!(db[buckets[0]].contains(
            &(Tuple {
                t: (key, key * key),
            })
        ));
    }
}

#[test]
fn test_replication() {
    let mut rng = rand::thread_rng();

    for i in 0..100 {
        let k = 12 + i + (rng.next_u32() % 8) as usize;
        let tuples: Vec<Tuple<usize, usize>> =
            (0..500 + i).map(|e| Tuple { t: (e, e * e) }).collect();

        let code = ReplicationCode::new(k);

        do_test(&code, k, &tuples);
    }
}

#[test]
fn test_sharding() {
    let mut rng = rand::thread_rng();

    for i in 0..100 {
        let k = 12 + i + (rng.next_u32() % 8) as usize;
        let tuples: Vec<Tuple<usize, usize>> = (0..500).map(|e| Tuple { t: (e, e * e) }).collect();

        let code = ShardingCode::new(k);
        do_test(&code, k, &tuples);
    }
}

#[test]
fn test_choices() {
    let mut rng = rand::thread_rng();

    for i in 0..100 {
        let k = 12 + i + (rng.next_u32() % 8) as usize;
        let tuples: Vec<Tuple<usize, usize>> =
            (0..500 + i).map(|e| Tuple { t: (e, e * e) }).collect();

        let code = ChoicesCode::new(k, 2);
        do_test(&code, k, &tuples);
    }
}

#[test]
fn test_cuckoo() {
    let mut rng = rand::thread_rng();

    for i in 0..100 {
        let k = 12 + i + (rng.next_u32() % 8) as usize;
        let tuples: Vec<Tuple<usize, usize>> =
            (0..500 + i).map(|e| Tuple { t: (e, e * e) }).collect();

        let code = CuckooCode::new(k, 3, 1.3);
        do_test(&code, k, &tuples);
    }
}


#[test]
fn test_pung() {
    let mut rng = rand::thread_rng();

    for i in 0..10 {
        let k = 12 + i + (rng.next_u32() % 8) as usize;
        let tuples: Vec<Tuple<usize, usize>> =
            (0..5000 + i).map(|e| Tuple { t: (e, e * e) }).collect();

        let mut code: PungCode<usize> = PungCode::new(k);

        // server
        let db: Vec<Vec<Tuple<usize, usize>>> = code.encode(&tuples);
        let keys: Vec<usize> = (0..k).collect();

        // Get label mapping
        let mut labels: HashMap<usize, Vec<usize>> = HashMap::new();

        for (i, vec_tuple) in db.iter().enumerate() {
            for tuple in vec_tuple {
                let entry = labels.entry(tuple.t.0).or_insert(Vec::new());
                entry.push(i);
            }
        }

        code.set_labels(labels);

        // client
        let _schedule = (&code as &BatchCode<usize, usize>)
            .get_schedule(&keys)
            .unwrap();
    }
}
