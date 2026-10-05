pub trait Progress: Send + Sync {
    fn stage(&self, name: &str, files: usize, bytes: u64);
    fn advance(&self, bytes: u64);
    fn file_done(&self);
    fn notice(&self, message: &str);
}

pub struct Silent;

impl Progress for Silent {
    fn stage(&self, _name: &str, _files: usize, _bytes: u64) {}
    fn advance(&self, _bytes: u64) {}
    fn file_done(&self) {}
    fn notice(&self, _message: &str) {}
}
