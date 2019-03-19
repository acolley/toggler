use chrono::{DateTime, Utc};
use diesel;
use diesel::RunQueryDsl;
use diesel::sqlite::SqliteConnection;
use failure::Error;
use failure_derive::Fail;
use serde::{Serialize, Deserialize};
use serde_json;
use uuid::Uuid;

use crate::database::models::{Event, NewEvent};
use crate::database::schema;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectId(Uuid);

impl ProjectId {
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Generation(u64);

impl Generation {
    pub fn first() -> Self {
        Self(0)
    }

    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }
}

#[derive(Debug, Eq, Fail, PartialEq)]
pub enum CreateProjectError {
    #[fail(display = "invalid project name: {}", name)]
    InvalidName {
        name: String,
    },
}

#[derive(Debug, Eq, Fail, PartialEq)]
pub enum ApplyEventError {
    #[fail(display = "invalid event `{}` applied to state `{}", event, state)]
    InvalidStateEvent {
        state: String,
        event: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    id: ProjectId,
    generation: Generation,
    name: String,
}

impl Project {
    pub fn create(id: ProjectId, name: &str) -> Result<Vec<ProjectEvent>, CreateProjectError> {
        Ok(vec![ProjectEvent::Created {
            id: id,
            name: String::from(name),
        }])
    }

    pub fn apply_event(project: Option<Self>, event: &ProjectEvent) -> Result<Self, ApplyEventError> {
        match (&project, event) {
            (None, ProjectEvent::Created {
                id, name,
            }) => Ok(Project {
                id: *id,
                generation: Generation(0),
                name: name.clone(),
            }),
            _ => Err(ApplyEventError::InvalidStateEvent {
                state: format!("{:?}", project),
                event: format!("{:?}", event),
            }),
        }
    }

    pub fn hydrate(events: &[ProjectEvent]) -> Result<Option<Self>, Error> {
        let mut project = None;
        for event in events {
            project = Some(Self::apply_event(project, event)?);
        }
        Ok(project)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProjectEvent {
    Created {
        id: ProjectId,
        name: String,
    },
}

impl ProjectEvent {
    pub fn type_(&self) -> String {
        match self {
            ProjectEvent::Created { .. } => "Created".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DomainEventId(Uuid);

impl DomainEventId {
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct DomainEvent {
    id: DomainEventId,
    project_id: ProjectId,
    created_at: DateTime<Utc>,
    event: ProjectEvent,
}

impl DomainEvent {
    pub fn from_event(event: Event) -> Result<Self, Error> {
        Ok(Self {
            id: DomainEventId(Uuid::parse_str(&event.id)?),
            project_id: ProjectId(Uuid::parse_str(&event.aggregate_id)?),
            created_at: event.created_at.parse::<DateTime<Utc>>()?,
            event: serde_json::from_str(&event.data)?,
        })
    }
}

pub struct DomainEvents<'a> {
    project_id: ProjectId,
    events: &'a [DomainEvent],
}

pub struct SqliteRepository<'a> {
    pub db: &'a SqliteConnection,
}

impl<'a> SqliteRepository<'a> {
    pub fn get(&self, id: ProjectId) -> Result<Option<Project>, Error> {
        use diesel::prelude::*;
        use crate::database::schema::events::dsl::{
            aggregate_id,
            events,
        };

        let results: Result<Vec<_>, Error> = events.filter(aggregate_id.eq(id.to_string()))
            .load::<Event>(self.db)?
            .into_iter()
            .map(DomainEvent::from_event)
            .map(|x| x.map(|e| e.event))
            .collect();
        Project::hydrate(&results?)
    }

    pub fn persist(&mut self, generation: Generation, events: &[DomainEvent]) -> Result<(), Error> {
        for event in events {
            let new = NewEvent {
                id: &event.id.to_string(),
                aggregate_id: &event.project_id.to_string(),
                created_at: &event.created_at.to_rfc3339(),
                type_: &event.event.type_(),
                data: &serde_json::to_string(&event.event)?,
            };
            diesel::insert_into(schema::events::table)
                .values(&new)
                .execute(self.db)?;
        }

        Ok(())
    }
}

pub struct CreateProject {
    pub name: String,
}

pub struct CreateProjectHandler<'a> {
    pub repository: &'a mut SqliteRepository<'a>,
    pub utc_now: fn() -> DateTime<Utc>,
}

impl<'a> CreateProjectHandler<'a> {
    pub fn handle(&mut self, command: &CreateProject) -> Result<(), Error> {
        let project_id = ProjectId(Uuid::new_v4());
        let events = Project::create(project_id, &command.name)?;
        let events: Vec<DomainEvent> = events.into_iter().map(|event| DomainEvent {
            id: DomainEventId(Uuid::new_v4()),
            project_id,
            created_at: (self.utc_now)(),
            event: event,
        }).collect();
        self.repository.persist(Generation::first(), &events)?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    mod project {
        use uuid::Uuid;

        use super::super::{
            Project,
            ProjectEvent,
            ProjectId,
        };

        #[test]
        fn test_create() {
            let id = ProjectId(Uuid::parse_str("936DA01F9ABD4d9d80C702AF85C822A8").unwrap());
            let events = Project::create(
                id,
                "test",
            );
            assert_eq!(events, Ok(vec![ProjectEvent::Created {
                id,
                name: "test".into(),
            }]));
        }
    }

    mod repository {
        use chrono::Utc;
        use chrono::offset::TimeZone;
        use diesel::prelude::*;
        use diesel::sqlite::SqliteConnection;
        use diesel_migrations;
        use failure::Error;
        use uuid::Uuid;

        use crate::database::schema;
        use crate::database::models::{Event, NewEvent};
        use crate::database::schema::events::dsl::*;

        use super::super::{
            DomainEvent,
            DomainEventId,
            Generation,
            Project,
            ProjectEvent,
            ProjectId,
            SqliteRepository,
        };

        #[test]
        fn test_get() -> Result<(), Error> {
            let db = &SqliteConnection::establish(":memory:")?;
            diesel_migrations::run_pending_migrations(db)?;
            let repository = SqliteRepository { db };
            let event = NewEvent {
                id: "550e8400-e29b-41d4-a716-446655440000",
                aggregate_id: "936da01f-9abd-4d9d-80c7-02af85c822a8",
                created_at: "2019-01-01T12:34:56+00:00",
                type_: "Created",
                data: "{\"Created\":{\"id\":\"936da01f-9abd-4d9d-80c7-02af85c822a8\",\"name\":\"test\"}}",
            };
            diesel::insert_into(schema::events::table)
                .values(&event)
                .execute(db)?;
            
            let project_id = ProjectId(
                Uuid::parse_str("936da01f-9abd-4d9d-80c7-02af85c822a8")?
            );
            let project = repository.get(project_id)?;

            assert_eq!(project, Some(Project {
                id: project_id,
                generation: Generation::first(),
                name: "test".to_owned(),
            }));
            Ok(())
        }

        #[test]
        fn test_persist() -> Result<(), Error> {
            let db = &SqliteConnection::establish(":memory:")?;
            diesel_migrations::run_pending_migrations(db)?;
            let mut repository = SqliteRepository { db };
            let project_id = ProjectId(
                Uuid::parse_str("936da01f-9abd-4d9d-80c7-02af85c822a8")?,
            );
            let event_id = DomainEventId(
                Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")?,
            );

            repository.persist(
                Generation::first(),
                &[
                    DomainEvent {
                        id: event_id,
                        project_id: project_id,
                        created_at: Utc.ymd(2019, 1, 1).and_hms(0, 0, 0),
                        event: ProjectEvent::Created {
                            id: project_id,
                            name: "test".into(),
                        },
                    },
                ],
            )?;

            let results = events.filter(id.eq("550e8400-e29b-41d4-a716-446655440000"))
                .load::<Event>(db)?;
            assert_eq!(results, vec![Event {
                id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
                aggregate_id: "936da01f-9abd-4d9d-80c7-02af85c822a8".to_owned(),
                created_at: "2019-01-01T00:00:00+00:00".to_owned(),
                type_: "Created".to_owned(),
                data: "{\"Created\":{\"id\":\"936da01f-9abd-4d9d-80c7-02af85c822a8\",\"name\":\"test\"}}".to_owned(),
            }]);
            Ok(())
        }
    }
}
