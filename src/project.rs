use std::str::FromStr;

use chrono::{DateTime, Utc};
use diesel;
use diesel::sqlite::SqliteConnection;
use diesel::RunQueryDsl;
use failure_derive::Fail;
use serde::{Deserialize, Serialize};
use serde_json;
use uuid::Uuid;

use crate::database::models::{Event, NewEvent};
use crate::database::schema;
use crate::domain::{Aggregate, DomainEvent, DomainEventId, Generation, Repository};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectId(Uuid);

impl ProjectId {
    pub fn to_string(&self) -> String {
        self.0.to_string()
    }
}

#[derive(Debug, Fail)]
pub enum ProjectIdParseError {
    #[fail(display = "fail to parse uuid")]
    UuidParseError(#[cause] uuid::parser::ParseError),
}

impl From<uuid::parser::ParseError> for ProjectIdParseError {
    fn from(e: uuid::parser::ParseError) -> ProjectIdParseError {
        ProjectIdParseError::UuidParseError(e)
    }
}

impl FromStr for ProjectId {
    type Err = ProjectIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let id = Uuid::parse_str(s)?;
        Ok(Self(id))
    }
}

impl From<ProjectId> for Uuid {
    fn from(id: ProjectId) -> Self {
        id.0
    }
}

#[derive(Debug, Eq, Fail, PartialEq)]
pub enum ProjectError {
    #[fail(display = "invalid project name: {}", name)]
    InvalidName { name: String },
    #[fail(display = "invalid event `{}` applied to state `{}", event, state)]
    InvalidStateEvent { state: String, event: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Project {
    pub id: ProjectId,
    pub generation: Generation,
    pub name: String,
}

impl Project {
    pub fn create(id: ProjectId, name: String) -> Result<Vec<ProjectEvent>, ProjectError> {
        Ok(vec![ProjectEvent::Created { id, name }])
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ProjectEvent {
    Created { id: ProjectId, name: String },
}

impl ProjectEvent {
    pub fn type_(&self) -> String {
        match self {
            ProjectEvent::Created { .. } => "Created".to_owned(),
        }
    }
}

impl Aggregate for Project {
    type Id = ProjectId;
    type Event = ProjectEvent;
    type Err = ProjectError;

    fn id(&self) -> &ProjectId {
        &self.id
    }

    fn apply_event(project: Option<Self>, event: &ProjectEvent) -> Result<Self, ProjectError> {
        match (&project, event) {
            (None, ProjectEvent::Created { id, name }) => Ok(Project {
                id: *id,
                generation: Generation::first(),
                name: name.clone(),
            }),
            _ => Err(ProjectError::InvalidStateEvent {
                state: format!("{:?}", project),
                event: format!("{:?}", event),
            }),
        }
    }
}

#[derive(Debug, Fail)]
pub enum DomainEventError {
    #[fail(display = "failed to parse uuid")]
    UuidParseError(#[cause] uuid::parser::ParseError),
    #[fail(display = "failed to parse datetime")]
    DateTimeParseError(#[cause] chrono::format::ParseError),
    #[fail(display = "failed to parse JSON data")]
    JsonParseError(#[cause] serde_json::error::Error),
}

impl From<uuid::parser::ParseError> for DomainEventError {
    fn from(e: uuid::parser::ParseError) -> Self {
        DomainEventError::UuidParseError(e)
    }
}

impl From<chrono::format::ParseError> for DomainEventError {
    fn from(e: chrono::format::ParseError) -> Self {
        DomainEventError::DateTimeParseError(e)
    }
}

impl From<serde_json::error::Error> for DomainEventError {
    fn from(e: serde_json::error::Error) -> Self {
        DomainEventError::JsonParseError(e)
    }
}

impl DomainEvent<Project> {
    pub fn from_event(event: Event) -> Result<Self, DomainEventError> {
        Ok(Self {
            id: DomainEventId::new(Uuid::parse_str(&event.id)?),
            aggregate_id: ProjectId(Uuid::parse_str(&event.aggregate_id)?),
            created_at: event.created_at.parse::<DateTime<Utc>>()?,
            event: serde_json::from_str(&event.data)?,
        })
    }
}

#[derive(Debug, Fail)]
pub enum SqliteRepositoryError {
    #[fail(display = "database error")]
    DatabaseError(#[cause] diesel::result::Error),
    #[fail(display = "domain event error")]
    DomainEventError(#[cause] DomainEventError),
    #[fail(display = "project error")]
    ProjectError(#[cause] ProjectError),
    #[fail(display = "json format error")]
    JsonFormatError(#[cause] serde_json::error::Error),
    #[fail(display = "not found error")]
    NotFoundError,
}

impl From<diesel::result::Error> for SqliteRepositoryError {
    fn from(e: diesel::result::Error) -> Self {
        SqliteRepositoryError::DatabaseError(e)
    }
}

impl From<DomainEventError> for SqliteRepositoryError {
    fn from(e: DomainEventError) -> Self {
        SqliteRepositoryError::DomainEventError(e)
    }
}

impl From<ProjectError> for SqliteRepositoryError {
    fn from(e: ProjectError) -> Self {
        SqliteRepositoryError::ProjectError(e)
    }
}

impl From<serde_json::error::Error> for SqliteRepositoryError {
    fn from(e: serde_json::error::Error) -> Self {
        SqliteRepositoryError::JsonFormatError(e)
    }
}

pub struct SqliteRepository<'a> {
    pub db: &'a SqliteConnection,
}

impl<'a> Repository for SqliteRepository<'a> {
    type Aggregate = Project;
    type Err = SqliteRepositoryError;

    fn get(&self, id: ProjectId) -> Result<Project, SqliteRepositoryError> {
        use crate::database::schema::events::dsl::{aggregate_id, events};
        use diesel::prelude::*;

        let results: Result<Vec<_>, DomainEventError> = events
            .filter(aggregate_id.eq(id.to_string()))
            .load::<Event>(self.db)?
            .into_iter()
            .map(DomainEvent::from_event)
            .map(|x| x.map(|e| e.event))
            .collect();
        let project = Project::hydrate(&results?)?;
        project.ok_or_else(|| SqliteRepositoryError::NotFoundError)
    }

    fn persist(
        &mut self,
        generation: Generation,
        events: &[DomainEvent<Project>],
    ) -> Result<(), SqliteRepositoryError> {
        let mut generation = generation;
        for event in events {
            let new = NewEvent {
                id: &event.id.to_string(),
                aggregate_id: &event.aggregate_id.to_string(),
                generation: generation.into(),
                created_at: &event.created_at.to_rfc3339(),
                type_: &event.event.type_(),
                data: &serde_json::to_string(&event.event)?,
            };
            diesel::insert_into(schema::events::table)
                .values(&new)
                .execute(self.db)?;
            generation = generation.next();
        }

        Ok(())
    }
}

pub struct CreateProject {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Fail)]
pub enum CreateProjectHandlerError {
    #[fail(display = "project error")]
    ProjectError(#[cause] ProjectError),
    #[fail(display = "repository error")]
    RepositoryError(#[cause] SqliteRepositoryError),
}

impl From<ProjectError> for CreateProjectHandlerError {
    fn from(e: ProjectError) -> Self {
        CreateProjectHandlerError::ProjectError(e)
    }
}

impl From<SqliteRepositoryError> for CreateProjectHandlerError {
    fn from(e: SqliteRepositoryError) -> Self {
        CreateProjectHandlerError::RepositoryError(e)
    }
}

pub struct CreateProjectHandler<'a> {
    pub repository: &'a mut SqliteRepository<'a>,
    pub utc_now: fn() -> DateTime<Utc>,
}

impl<'a> CreateProjectHandler<'a> {
    pub fn handle(&mut self, command: CreateProject) -> Result<Project, CreateProjectHandlerError> {
        let project_id = ProjectId(command.id);
        let events = Project::create(project_id, command.name)?;
        let project = Project::hydrate(&events)?.expect("Project is not None");
        let events: Vec<DomainEvent<Project>> = events
            .into_iter()
            .map(|event| DomainEvent {
                id: DomainEventId::new(Uuid::new_v4()),
                aggregate_id: project_id,
                created_at: (self.utc_now)(),
                event,
            })
            .collect();
        self.repository.persist(Generation::first(), &events)?;
        Ok(project)
    }
}

pub struct ListProject {
    pub id: ProjectId,
}

#[derive(Debug, Fail)]
pub enum ListProjectHandlerError {
    #[fail(display = "repository error")]
    RepositoryError(#[cause] SqliteRepositoryError),
}

impl From<SqliteRepositoryError> for ListProjectHandlerError {
    fn from(e: SqliteRepositoryError) -> Self {
        ListProjectHandlerError::RepositoryError(e)
    }
}

pub struct ListProjectHandler<'a> {
    pub repository: &'a SqliteRepository<'a>,
}

impl<'a> ListProjectHandler<'a> {
    pub fn handle(&self, command: ListProject) -> Result<Project, ListProjectHandlerError> {
        Ok(self.repository.get(command.id)?)
    }
}

#[cfg(test)]
mod test {
    mod project {
        use uuid::Uuid;

        use super::super::{Project, ProjectEvent, ProjectId};

        #[test]
        fn test_create() {
            let id = ProjectId(Uuid::parse_str("936DA01F9ABD4d9d80C702AF85C822A8").unwrap());
            let events = Project::create(id, "test".to_owned());
            assert_eq!(
                events,
                Ok(vec![ProjectEvent::Created {
                    id,
                    name: "test".into(),
                }])
            );
        }
    }

    mod repository {
        use chrono::offset::TimeZone;
        use chrono::Utc;
        use diesel::prelude::*;
        use diesel::sqlite::SqliteConnection;
        use diesel_migrations;
        use failure::Error;
        use uuid::Uuid;

        use crate::database::models::{Event, NewEvent};
        use crate::database::schema;
        use crate::database::schema::events::dsl::*;
        use crate::domain::Repository;

        use super::super::{
            DomainEvent, DomainEventId, Generation, Project, ProjectEvent, ProjectId,
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
                generation: 0,
                created_at: "2019-01-01T12:34:56+00:00",
                type_: "Created",
                data: "{\"Created\":{\"id\":\"936da01f-9abd-4d9d-80c7-02af85c822a8\",\"name\":\"test\"}}",
            };
            diesel::insert_into(schema::events::table)
                .values(&event)
                .execute(db)?;

            let project_id = ProjectId(Uuid::parse_str("936da01f-9abd-4d9d-80c7-02af85c822a8")?);
            let project = repository.get(project_id)?;

            assert_eq!(
                project,
                Project {
                    id: project_id,
                    generation: Generation::first(),
                    name: "test".to_owned(),
                },
            );
            Ok(())
        }

        #[test]
        fn test_persist() -> Result<(), Error> {
            let db = &SqliteConnection::establish(":memory:")?;
            diesel_migrations::run_pending_migrations(db)?;
            let mut repository = SqliteRepository { db };
            let project_id = ProjectId(Uuid::parse_str("936da01f-9abd-4d9d-80c7-02af85c822a8")?);
            let event_id = DomainEventId::new(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000")?);

            repository.persist(
                Generation::first(),
                &[DomainEvent {
                    id: event_id,
                    aggregate_id: project_id,
                    created_at: Utc.ymd(2019, 1, 1).and_hms(0, 0, 0),
                    event: ProjectEvent::Created {
                        id: project_id,
                        name: "test".into(),
                    },
                }],
            )?;

            let results = events
                .filter(id.eq("550e8400-e29b-41d4-a716-446655440000"))
                .load::<Event>(db)?;
            assert_eq!(results, vec![Event {
                id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
                aggregate_id: "936da01f-9abd-4d9d-80c7-02af85c822a8".to_owned(),
                generation: 0,
                created_at: "2019-01-01T00:00:00+00:00".to_owned(),
                type_: "Created".to_owned(),
                data: "{\"Created\":{\"id\":\"936da01f-9abd-4d9d-80c7-02af85c822a8\",\"name\":\"test\"}}".to_owned(),
            }]);
            Ok(())
        }
    }
}
