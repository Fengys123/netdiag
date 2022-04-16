use etherparse::TcpHeader;
use std::time::Instant;

#[derive(Debug)]
pub struct Reply {
    pub head: TcpHeader,
    pub when: Instant,
}

impl Reply {
    pub fn new(head: TcpHeader, when: Instant) -> Self {
        Self { head, when }
    }
}
