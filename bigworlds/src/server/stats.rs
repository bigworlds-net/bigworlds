use crate::time::Instant;

#[derive(Clone)]
pub struct Stats {
    /// Time when last message was received.
    pub last_msg_time: Instant,
    /// Time when last new client connection got accepted.
    pub last_accept_time: Instant,

    /// Average response time across all processed requests.
    pub average_response_time_ms: u64,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            last_msg_time: Instant::now(),
            last_accept_time: Instant::now(),
            average_response_time_ms: 0,
        }
    }
}
