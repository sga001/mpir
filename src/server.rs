use sealpir::server::PirServer;
use sealpir::{PirQuery, PirReply};

pub struct MultiPirServer<'a> {
    handles: Vec<PirServer<'a>>,
}

impl<'a> MultiPirServer<'a> {

    pub fn new(buckets: &[(u32, u32)], poly_degree: u32, log_plain: u32) -> MultiPirServer<'a> {
        let mut handles = Vec::with_capacity(buckets.len());

        for &(ele_num, ele_size) in buckets {
            handles.push(PirServer::new(ele_num, ele_size, poly_degree, log_plain));
        }

        MultiPirServer { handles }
    }

    pub fn update_params(&mut self, buckets: &[(u32, u32)]) {
        assert_eq!(buckets.len(), self.handles.len());

        for (i, handle) in self.handles.iter_mut().enumerate() {
            handle.update_params(buckets[i].0, buckets[i].1);
        }
    }

    pub fn set_galois_key(&mut self, key: &[u8], client_id: u32) {
        for handle in self.handles.iter_mut() {
            handle.set_galois_key(key, client_id);
        }
    }

    pub fn setup<T>(&mut self, collection: &[Vec<T>]) {
        assert_eq!(collection.len(), self.handles.len());

        for (i, handle) in self.handles.iter_mut().enumerate() {
            handle.setup(&collection[i]);
        }
    }

    pub fn gen_replies(&self, queries: &[PirQuery], client_id: u32) -> Vec<PirReply> {
        let len = queries.len();
        assert_eq!(len, self.handles.len());

        let mut answers = Vec::with_capacity(len);

        for (i, query) in queries.iter().enumerate() {
            answers.push(self.handles[i].gen_reply(query, client_id));
        }

        answers
    }
}
