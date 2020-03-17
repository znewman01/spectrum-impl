pub mod discovery;
pub mod health;
pub mod quorum;
mod retry;

use crate::proto::{ClientId, WorkerId};
use crate::protocols::insecure;

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct Group {
    pub idx: u16,
    _private: (),
}

impl Group {
    pub fn new(idx: u16) -> Self {
        Group { idx, _private: () }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct LeaderInfo {
    pub group: Group,
    _private: (),
}

impl LeaderInfo {
    pub fn new(group: Group) -> Self {
        LeaderInfo {
            group,
            _private: (),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct WorkerInfo {
    pub group: Group,
    pub idx: u16,
    _private: (),
}

impl WorkerInfo {
    pub fn new(group: Group, idx: u16) -> Self {
        WorkerInfo {
            group,
            idx,
            _private: (),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Default)]
pub struct PublisherInfo {
    _private: (),
}

impl PublisherInfo {
    pub fn new() -> Self {
        PublisherInfo::default()
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Default)]
pub struct ClientInfo {
    pub idx: u16,
    pub broadcast: Option<(u8, insecure::ChannelKey)>,
    _private: (),
}

impl ClientInfo {
    pub fn new(idx: u16) -> Self {
        ClientInfo {
            idx,
            broadcast: None,
            _private: (),
        }
    }

    pub fn new_broadcaster(idx: u16, message: u8, key: insecure::ChannelKey) -> Self {
        ClientInfo {
            idx,
            broadcast: Some((message, key)),
            _private: (),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub enum Service {
    Leader(LeaderInfo),
    Publisher(PublisherInfo),
    Worker(WorkerInfo),
    Client(ClientInfo),
}

impl From<LeaderInfo> for Service {
    fn from(info: LeaderInfo) -> Self {
        Service::Leader(info)
    }
}

impl From<WorkerInfo> for Service {
    fn from(info: WorkerInfo) -> Self {
        Service::Worker(info)
    }
}

impl From<PublisherInfo> for Service {
    fn from(info: PublisherInfo) -> Self {
        Service::Publisher(info)
    }
}

impl From<ClientInfo> for Service {
    fn from(info: ClientInfo) -> Self {
        Service::Client(info)
    }
}

impl ClientInfo {
    // TODO(zjn): merge these more closely?
    pub fn to_proto(&self) -> ClientId {
        ClientId {
            client_id: self.idx.to_string(),
        }
    }
}

impl From<WorkerInfo> for WorkerId {
    fn from(info: WorkerInfo) -> WorkerId {
        WorkerId {
            group: info.group.idx as u32,
            idx: info.idx as u32,
        }
    }
}

// TODO(zjn): maybe TryFrom and cast safely?
impl From<WorkerId> for WorkerInfo {
    fn from(worker: WorkerId) -> WorkerInfo {
        WorkerInfo::new(Group::new(worker.group as u16), worker.idx as u16)
    }
}

impl From<&ClientId> for ClientInfo {
    fn from(client: &ClientId) -> ClientInfo {
        // TODO(zjn): change proto type of client_id from string to uint32
        ClientInfo::new(client.client_id.parse().expect("Should parse as number"))
    }
}
