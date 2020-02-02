pub mod discovery;
pub mod health;
pub mod quorum;
mod retry;

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct Group(pub u16);

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct LeaderInfo {
    pub group: Group,
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct WorkerInfo {
    pub group: Group,
    pub idx: u16,
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct PublisherInfo {}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub enum Service {
    Leader(LeaderInfo),
    Publisher(PublisherInfo),
    Worker(WorkerInfo),
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
