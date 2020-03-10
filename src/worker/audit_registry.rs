use crate::{services::ClientInfo};

use std::iter::FromIterator;
use tokio::sync::{Mutex, RwLock};

// For each client, an Option of lists of audit shares (with appropriate locking).
//
// The idea is that you add shares for each client as received, then drain all
// of them (one-time only) to check the audit.
//
// TODO(zjn): do you need the inner mutex?
pub struct AuditRegistry<S>(Vec<RwLock<Option<Mutex<Vec<S>>>>>);

impl<S> AuditRegistry<S> {
    pub fn new(num_clients: u16) -> AuditRegistry<S> {
        let mut vec = vec![];
        for _ in 0..num_clients {
            vec.push(RwLock::new(Some(Mutex::new(vec![]))));
        }
        AuditRegistry(vec)
    }

    pub async fn drain(&self, info: ClientInfo) -> Vec<S> {
        let mut opt_lock = self.0[info.idx as usize].write().await;
        let vec_lock = opt_lock.take().expect("May only drain once.");
        let mut vec = vec_lock.lock().await;
        Vec::from_iter(vec.drain(..))
    }

    pub async fn add(&self, info: ClientInfo, value: S) -> usize {
        let opt_lock = self.0[info.idx as usize].read().await;
        let vec_lock = opt_lock
            .as_ref()
            .expect("Can only add to client that hasn't had its shares drained.");
        let mut vec = vec_lock.lock().await;
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

    #[tokio::test]
    async fn test_audit_registry_empty() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = AuditRegistry::<()>::new(NUM_CLIENTS);

        for client in clients {
            let shares = reg.drain(client).await;
            assert!(shares.is_empty());
        }
    }

    #[tokio::test]
    async fn test_audit_registry_put_shares() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = AuditRegistry::<()>::new(NUM_CLIENTS);
        let expected_shares = vec![(); NUM_SHARES as usize];

        for client in &clients {
            for (idx, share) in expected_shares.iter().enumerate() {
                assert_eq!(reg.add(*client, share.clone()).await, idx + 1);
            }
        }

        for client in clients {
            assert_eq!(reg.drain(client).await, expected_shares);
        }
    }

    #[should_panic]
    #[tokio::test]
    async fn test_audit_registry_drain_twice_panics() {
        let mut clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = AuditRegistry::<()>::new(NUM_CLIENTS);

        for client in &clients {
            reg.drain(*client).await;
        }

        reg.drain(clients.pop().unwrap()).await;
    }
}
