use crate::proto::ShareCheck;
use crate::services::ClientInfo;

use std::iter::FromIterator;
use tokio::sync::{Mutex, RwLock};

pub struct CheckRegistry(Vec<RwLock<Option<Mutex<Vec<ShareCheck>>>>>);

impl CheckRegistry {
    pub fn new(num_clients: u16) -> CheckRegistry {
        let mut vec = vec![];
        for _ in 0..num_clients {
            vec.push(RwLock::new(Some(Mutex::new(vec![]))));
        }
        CheckRegistry(vec)
    }

    pub async fn drain(&self, info: ClientInfo) -> Vec<ShareCheck> {
        let mut opt_lock = self.0[info.idx as usize].write().await;
        let vec_lock = opt_lock.take().expect("May only drain once.");
        let mut vec = vec_lock.lock().await;
        Vec::from_iter(vec.drain(..))
    }

    pub async fn add(&self, info: ClientInfo, value: ShareCheck) -> usize {
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
    use super::*;

    const NUM_CLIENTS: u16 = 10;
    const NUM_SHARES: u16 = 100;

    #[tokio::test]
    async fn test_check_registry_empty() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = CheckRegistry::new(NUM_CLIENTS);

        for client in clients {
            let shares = reg.drain(client).await;
            assert!(shares.is_empty());
        }
    }

    #[tokio::test]
    async fn test_check_registry_put_shares() {
        let clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = CheckRegistry::new(NUM_CLIENTS);
        let expected_shares = vec![ShareCheck::default(); NUM_SHARES as usize];

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
    async fn test_check_registry_drain_twice_panics() {
        let mut clients: Vec<ClientInfo> = (0..NUM_CLIENTS).map(ClientInfo::new).collect();
        let reg = CheckRegistry::new(NUM_CLIENTS);

        for client in &clients {
            reg.drain(*client).await;
        }

        reg.drain(clients.pop().unwrap()).await;
    }
}
