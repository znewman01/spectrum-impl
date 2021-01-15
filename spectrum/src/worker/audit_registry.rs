// https://github.com/rust-lang/rust-clippy/issues/5902
#![allow(clippy::same_item_push)]
use crate::services::ClientInfo;

use tokio::sync::Mutex;

// Starts as Some((None, vec![])).
// The first member of the inner-tuple corresponds to the write token; it's initialized by init().
// The second member corresponds to each audit share.
// drain() takes the whole thing.
type ClientAuditState<S, T> = Option<(Option<T>, Vec<S>)>;

// For each client, an Option of lists of audit shares (with appropriate locking).
//
// The idea is that you add shares for each client as received, then drain all
// of them (one-time only) to check the audit.
//
// Each entry also stores the write token for the client.
pub struct AuditRegistry<S, T>(Vec<Mutex<ClientAuditState<S, T>>>);

impl<S, T> AuditRegistry<S, T> {
    pub fn new(num_clients: u16, num_parties: u16) -> AuditRegistry<S, T> {
        let mut vec = Vec::with_capacity(num_clients as usize);
        for _ in 0..num_clients {
            vec.push(Mutex::new(Some((
                None,
                Vec::with_capacity(num_parties as usize),
            ))));
        }
        AuditRegistry(vec)
    }

    pub async fn init(&self, info: &ClientInfo, token: T) {
        let mut lock = self.0[info.idx as usize].lock().await;
        let (token_holder, _) = lock.as_mut().expect("Cannot init() a drain()ed registry.");
        (*token_holder) = Some(token);
    }

    pub async fn drain(&self, info: &ClientInfo) -> (T, Vec<S>) {
        let mut lock = self.0[info.idx as usize].lock().await;
        let (token, mut vec): (Option<T>, Vec<S>) = lock.take().expect("May only drain once.");
        let token = token.expect("Should only call drain() after init().");
        let shares: Vec<_> = vec.drain(..).collect();
        (token, shares)
    }

    pub async fn add(&self, info: &ClientInfo, value: S) -> usize {
        let mut lock = self.0[info.idx as usize].lock().await;
        let vec = lock
            .as_mut()
            .map(|(_, x)| x)
            .expect("Can only add to client that hasn't had its shares drained.");
        vec.push(value);
        vec.len()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unit_arg)]
    use super::*;

    const NUM_CLIENTS: u16 = 10;
    const NUM_SHARES: u16 = 100;

    #[should_panic]
    #[tokio::test]
    async fn test_audit_registry_bad_client_idx() {
        let client = ClientInfo::new(0);
        let reg = AuditRegistry::<(), u16>::new(0, NUM_SHARES);
        reg.drain(&client).await;
    }

    #[should_panic]
    #[tokio::test]
    async fn test_audit_registry_drain_before_init() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = AuditRegistry::<(), u16>::new(NUM_CLIENTS, NUM_SHARES);

        reg.drain(&clients[0]).await;
    }

    #[tokio::test]
    async fn test_audit_registry_empty() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = AuditRegistry::<(), u16>::new(NUM_CLIENTS, NUM_SHARES);

        for client in &clients {
            let expected_value = client.idx;
            reg.init(client, expected_value).await;
            let (actual_value, shares) = reg.drain(client).await;
            assert_eq!(actual_value, expected_value);
            assert!(shares.is_empty());
        }
    }

    #[tokio::test]
    async fn test_audit_registry_put_shares() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = AuditRegistry::<(), u16>::new(NUM_CLIENTS, NUM_SHARES);
        let expected_shares = vec![(); NUM_SHARES as usize];

        for client in &clients {
            for (idx, share) in expected_shares.iter().enumerate() {
                assert_eq!(reg.add(client, *share).await, idx + 1);
            }
        }

        for client in &clients {
            let expected_value = client.idx;
            reg.init(client, expected_value).await;
            let (actual_value, shares) = reg.drain(client).await;
            assert_eq!(actual_value, expected_value);
            assert_eq!(shares, expected_shares);
        }
    }

    #[should_panic]
    #[tokio::test]
    async fn test_audit_registry_drain_twice_panics() {
        let mut clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = AuditRegistry::<(), u16>::new(NUM_CLIENTS, NUM_SHARES);

        for client in &clients {
            let expected_value = client.idx;
            reg.init(client, expected_value).await;
            reg.drain(client).await;
        }

        reg.drain(&clients.pop().unwrap()).await;
    }
}
