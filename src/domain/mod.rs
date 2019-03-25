use std::fmt::Debug;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Generation(i32);

impl Generation {
    pub fn first() -> Self {
        Self(0)
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }
}

impl From<Generation> for i32 {
    fn from(generation: Generation) -> Self {
        generation.0
    }
}

pub trait Aggregate {
    type Id: Debug + Eq + PartialEq;
    type Event: Debug + Eq + PartialEq;

    fn id(&self) -> &Self::Id;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainEventId(Uuid);

impl DomainEventId {
    pub fn new(id: Uuid) -> Self {
        DomainEventId(id)
    }

    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DomainEvent<T: Aggregate> {
    pub id: DomainEventId,
    pub aggregate_id: <T as Aggregate>::Id,
    pub created_at: DateTime<Utc>,
    pub event: <T as Aggregate>::Event,
}

pub trait Repository {
    type Aggregate: Aggregate;
    type Error;

    fn get(&self, id: <<Self as Repository>::Aggregate as Aggregate>::Id) -> Result<Self::Aggregate, Self::Error>;
    fn persist(&mut self, generation: Generation, events: &[DomainEvent<Self::Aggregate>]) -> Result<(), Self::Error>;
}
