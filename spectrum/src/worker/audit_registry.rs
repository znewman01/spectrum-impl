// https://github.com/rust-lang/rust-clippy/issues/5902
#![allow(clippy::same_item_push)]
use crate::services::ClientInfo;
use log::warn;
use std::collections::HashMap;

use tokio::sync::Mutex;

// The first member of the inner-tuple corresponds to the write token; it's initialized by init().
// The second member corresponds to each audit share.
// drain() takes the whole thing.
#[derive(Debug)]
struct ClientAuditState<S, T> {
    pub write_token: Option<T>,
    audit_shares: Vec<S>,
}

/// A complete client audit, ready to be checked.
pub struct ClientAudit<S, T> {
    pub write_token: T,
    pub audit_shares: Vec<S>,
}

impl<S, T> From<ClientAuditState<S, T>> for ClientAudit<S, T> {
    fn from(rhs: ClientAuditState<S, T>) -> Self {
        ClientAudit::<S, T> {
            write_token: rhs.write_token.unwrap(),
            audit_shares: rhs.audit_shares,
        }
    }
}

impl<S, T> ClientAuditState<S, T> {
    fn new(write_token: Option<T>, audit_shares: Vec<S>) -> Self {
        ClientAuditState {
            write_token,
            audit_shares,
        }
    }
}

// For each client, an Option of lists of audit shares (with appropriate locking).
//
// The idea is that you add shares for each client as received, then drain all
// of them (one-time only) to check the audit.
//
// Each entry also stores the write token for the client.
pub struct AuditRegistry<S, T> {
    registry: HashMap<ClientInfo, Mutex<ClientAuditState<S, T>>>,
    num_parties: u16,
}

impl<S, T> AuditRegistry<S, T> {
    pub fn new(num_clients: u128, num_parties: u16) -> AuditRegistry<S, T> {
        AuditRegistry {
            registry: HashMap::with_capacity(num_clients as usize),
            num_parties,
        }
    }

    pub async fn init(&mut self, info: &ClientInfo, token: T) {
        if let Some(mutex) = self.registry.get(info) {
            let mut state = mutex.lock().await;
            if !state.audit_shares.is_empty() {
                warn!("Re-init before drain.")
            }
            state.write_token.replace(token);
        } else {
            let vec = Vec::with_capacity(self.num_parties as usize);
            self.registry.insert(
                info.clone(),
                Mutex::new(ClientAuditState::new(Some(token), vec)),
            );
        }
    }

    pub async fn drain(&mut self, info: &ClientInfo) -> ClientAudit<S, T> {
        if let Some(mutex) = self.registry.remove(info) {
            return mutex.into_inner().into();
        } else {
            panic!("May only drain once, and must be after init'd.");
        }
    }

    pub async fn add(&mut self, info: &ClientInfo, value: S) -> usize {
        if let Some(lock) = self.registry.get(info) {
            let mut guard = lock.lock().await;
            guard.audit_shares.push(value);
            guard.audit_shares.len()
        } else {
            let mut vec = Vec::with_capacity(self.num_parties as usize);
            vec.push(value);
            let client_state = ClientAuditState::new(None, vec);
            self.registry.insert(info.clone(), Mutex::new(client_state));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unit_arg)]
    use super::*;

    const NUM_CLIENTS: u128 = 10;
    const NUM_SHARES: u16 = 100;

    #[should_panic]
    #[tokio::test]
    async fn test_audit_registry_bad_client_idx() {
        let client = ClientInfo::new(0);
        let mut reg = AuditRegistry::<(), ()>::new(0, NUM_SHARES);
        reg.drain(&client).await;
    }

    #[should_panic]
    #[tokio::test]
    async fn test_audit_registry_drain_before_init() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let mut reg = AuditRegistry::<(), ()>::new(NUM_CLIENTS, NUM_SHARES);

        reg.drain(&clients[0]).await;
    }

    #[tokio::test]
    async fn test_audit_registry_empty() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let mut reg = AuditRegistry::<(), u128>::new(NUM_CLIENTS, NUM_SHARES);

        for client in &clients {
            let expected_value = client.idx;
            reg.init(client, expected_value).await;
            let state = reg.drain(client).await;
            assert_eq!(state.write_token, expected_value);
            assert!(state.audit_shares.is_empty());
        }
    }

    #[tokio::test]
    async fn test_audit_registry_put_shares() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let mut reg = AuditRegistry::<(), u128>::new(NUM_CLIENTS, NUM_SHARES);
        let expected_shares = vec![(); NUM_SHARES as usize];

        for client in &clients {
            for (idx, share) in expected_shares.iter().enumerate() {
                assert_eq!(reg.add(client, *share).await, idx + 1);
            }
        }

        for client in &clients {
            let expected_value = client.idx;
            reg.init(client, expected_value).await;
            let state = reg.drain(client).await;
            assert_eq!(state.write_token, expected_value);
            assert_eq!(state.audit_shares, expected_shares);
        }
    }

    #[should_panic]
    #[tokio::test]
    async fn test_audit_registry_drain_twice_panics() {
        let mut clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let mut reg = AuditRegistry::<(), u128>::new(NUM_CLIENTS, NUM_SHARES);

        for client in &clients {
            let expected_value = client.idx;
            reg.init(client, expected_value).await;
            reg.drain(client).await;
        }

        reg.drain(&clients.pop().unwrap()).await;
    }

    #[tokio::test]
    async fn test_audit_registry_can_reinit_after_drain() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let mut reg = AuditRegistry::<(), u128>::new(NUM_CLIENTS, NUM_SHARES);

        for client in &clients {
            let expected_value = client.idx;
            reg.init(client, expected_value).await;
            assert_eq!(reg.drain(client).await.write_token, expected_value);
            reg.init(client, expected_value + 1).await;
            assert_eq!(reg.drain(client).await.write_token, expected_value + 1);
        }
    }

    #[tokio::test]
    async fn test_audit_registry_can_reinit_no_drain() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let mut reg = AuditRegistry::<(), u128>::new(NUM_CLIENTS, NUM_SHARES);

        for client in &clients {
            let expected_value = client.idx;
            reg.init(client, expected_value).await;
            reg.init(client, expected_value + 1).await;
            assert_eq!(reg.drain(client).await.write_token, expected_value + 1);
        }
    }
}
