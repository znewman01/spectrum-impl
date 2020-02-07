pub mod discovery;
pub mod health;
pub mod quorum;
mod retry;

use crate::proto::{ClientId, WorkerId};

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

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Default)]
pub struct ClientInfo {
    pub idx: u16,
    _private: (),
}

impl ClientInfo {
    pub fn new(idx: u16) -> Self {
        ClientInfo { idx, _private: () }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
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

// TODO(zjn): merge these more closely
impl From<ClientInfo> for ClientId {
    fn from(info: ClientInfo) -> ClientId {
        ClientId {
            client_id: info.idx.to_string(),
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
