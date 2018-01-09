use sealpir::client::PirClient;
use sealpir::{PirReply, PirQuery};

pub struct MultiPirClient<'a> {
    handles: Vec<PirClient<'a>>,
}

impl<'a> MultiPirClient<'a> {

    pub fn new(buckets: &[(u32, u32)], poly_degree: u32, log_plain_mod: u32, d: u32) -> MultiPirClient<'a> {
        let mut handles = Vec::with_capacity(buckets.len());

        for &(ele_num, ele_size) in buckets {
            handles.push(PirClient::new(ele_num, ele_size, poly_degree, log_plain_mod, d));
        }

        MultiPirClient { handles }
    }

    pub fn update_params(&mut self, buckets: &[(u32, u32)], d: u32) {
        assert_eq!(buckets.len(), self.handles.len());

        for (i, handle) in self.handles.iter_mut().enumerate() {
            handle.update_params(buckets[i].0, buckets[i].1, d);
        }
    }

    pub fn gen_query(&self, indexes: &[u32]) -> Vec<PirQuery> {
        let len = indexes.len();
        assert_eq!(len, self.handles.len());

        let mut queries = Vec::with_capacity(len);

        for (i, index) in indexes.iter().enumerate() {
            queries.push(self.handles[i].gen_query(*index));
        }

        queries
    }

    pub fn get_galois_keys(&self) -> Vec<Vec<u8>> {
        let mut keys = Vec::with_capacity(self.handles.len());

        for handle in &self.handles {
            keys.push(handle.get_key().clone());
        }

        keys
    }

    pub fn decode_replies<T: Clone>(&self, indexes: &[u32], replies: &[PirReply]) -> Vec<T> {
        let len = replies.len();
        assert_eq!(len, self.handles.len());

        let mut results = Vec::with_capacity(len);

        for (i, handle) in self.handles.iter().enumerate() {
            results.push(handle.decode_reply(indexes[i], &replies[i]));
        }

        results
    }
}
