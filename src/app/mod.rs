use actix::{Actor, Addr, Handler, Message, SyncArbiter, SyncContext};
use actix_web::middleware::Logger;
use actix_web::AsyncResponder;
use actix_web::{
    dev::FromParam, error::ResponseError, http::StatusCode, server, server::HttpServer,
    HttpResponse, Json, Path, State,
};
use actix_web::{http::Method, App};
use chrono::Utc;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use diesel::Connection;
use failure::Error;
use failure_derive::Fail;
use futures::Future;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::project;
use crate::project::{
    error::{
        CreateProjectHandlerError, ListProjectHandlerError, ProjectIdParseError,
        SqliteRepositoryError,
    },
    CreateProjectHandler, ListProjectHandler, ProjectId, SqliteRepository,
};

impl FromParam for ProjectId {
    type Err = ProjectIdParseError;

    fn from_param(s: &str) -> Result<Self, Self::Err> {
        s.parse()
    }
}

impl ResponseError for ProjectIdParseError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            ProjectIdParseError::UuidParseError(_) => HttpResponse::new(StatusCode::BAD_REQUEST),
        }
    }
}

struct Environment {
    id: Uuid,
    name: String,
}

struct Feature {
    id: Uuid,
    name: String,
    retired: bool,
}

struct Toggle {
    id: Uuid,
    feature_id: Uuid,
    version: i32,
    retired: bool,
}

#[derive(Debug)]
pub struct Variant {
    id: Uuid,
    generation: u64,
    toggle_id: Uuid,
    name: String,
    retired: bool,
}

#[derive(Clone, Debug)]
pub enum VariantEvent {
    Created {
        id: Uuid,
        toggle_id: Uuid,
        name: String,
    },
    Renamed(String),
    Retired,
    Revived,
}

#[derive(Debug, Fail)]
pub enum AppError {
    #[fail(display = "database pool error")]
    DatabasePoolError(#[cause] r2d2::Error),
    #[fail(display = "database error")]
    DatabaseError(#[cause] diesel::result::Error),
    #[fail(display = "mailbox error")]
    MailboxError(#[cause] actix::MailboxError),
    #[fail(display = "json payload error")]
    JsonPayloadError(#[cause] actix_web::error::JsonPayloadError),
    #[fail(display = "create project error")]
    CreateProjectError(#[cause] CreateProjectHandlerError),
    #[fail(display = "list project error")]
    ListProjectError(#[cause] ListProjectHandlerError),
}

impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self {
        AppError::DatabasePoolError(e)
    }
}

impl From<diesel::result::Error> for AppError {
    fn from(e: diesel::result::Error) -> Self {
        AppError::DatabaseError(e)
    }
}

impl From<actix::MailboxError> for AppError {
    fn from(e: actix::MailboxError) -> Self {
        AppError::MailboxError(e)
    }
}

impl From<actix_web::error::JsonPayloadError> for AppError {
    fn from(e: actix_web::error::JsonPayloadError) -> Self {
        AppError::JsonPayloadError(e)
    }
}

impl From<CreateProjectHandlerError> for AppError {
    fn from(e: CreateProjectHandlerError) -> Self {
        AppError::CreateProjectError(e)
    }
}

impl From<ListProjectHandlerError> for AppError {
    fn from(e: ListProjectHandlerError) -> Self {
        AppError::ListProjectError(e)
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        match *self {
            AppError::DatabasePoolError(_) => HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR),
            AppError::DatabaseError(_) => HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR),
            AppError::MailboxError(_) => HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR),
            AppError::JsonPayloadError(_) => HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR),
            AppError::CreateProjectError(_) => HttpResponse::new(StatusCode::BAD_REQUEST),
            AppError::ListProjectError(ListProjectHandlerError::RepositoryError(
                SqliteRepositoryError::NotFoundError,
            )) => HttpResponse::new(StatusCode::NOT_FOUND),
            AppError::ListProjectError(_) => HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }
}

pub struct Executor {
    pub db: Pool<ConnectionManager<SqliteConnection>>,
}

impl Actor for Executor {
    type Context = SyncContext<Self>;
}

#[derive(Clone)]
pub struct AppState {
    pub executor: Addr<Executor>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateProject {
    pub name: String,
}

impl Message for CreateProject {
    type Result = Result<project::Project, AppError>;
}

impl Handler<CreateProject> for Executor {
    type Result = Result<project::Project, AppError>;

    fn handle(&mut self, msg: CreateProject, _: &mut Self::Context) -> Self::Result {
        let db = &self.db.get().map_err(|e| -> AppError { e.into() })?;
        db.transaction::<_, AppError, _>(|| {
            let repository = &mut SqliteRepository { db };
            let handler = &mut CreateProjectHandler {
                repository,
                utc_now: Utc::now,
            };

            let project = handler
                .handle(project::CreateProject {
                    id: Uuid::new_v4(),
                    name: msg.name,
                })
                .map_err(|e| -> AppError { e.into() })?;
            Ok(project)
        })
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct ListProject {
    id: ProjectId,
}

impl Message for ListProject {
    type Result = Result<project::Project, AppError>;
}

impl Handler<ListProject> for Executor {
    type Result = Result<project::Project, AppError>;

    fn handle(&mut self, msg: ListProject, _: &mut Self::Context) -> Self::Result {
        let db = &self.db.get().map_err(|e| -> AppError { e.into() })?;
        db.transaction::<_, AppError, _>(|| {
            let repository = &mut SqliteRepository { db };
            let handler = &mut ListProjectHandler { repository };

            let project = handler
                .handle(project::ListProject { id: msg.id })
                .map_err(|e| -> AppError { e.into() })?;
            Ok(project)
        })
    }
}

// Based on examples: https://github.com/actix/examples/blob/d3a69f0c58f2df583adea59a79969a8c23a03a2a/diesel/src/main.rs
pub fn create_project(
    (body, state): (Json<CreateProject>, State<AppState>),
) -> impl Future<Item = Json<Project>, Error = AppError> {
    state
        .executor
        .send(CreateProject {
            name: body.name.clone(),
        })
        .from_err()
        .and_then(|res| res.map(|x| Json(x.into())))
        .responder()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Project {
    id: Uuid,
    name: String,
}

/// Domain Project to DTO Project
impl From<project::Project> for Project {
    fn from(p: project::Project) -> Self {
        Self {
            id: p.id.into(),
            name: p.name,
        }
    }
}

pub fn list_project(
    (id, state): (Path<ProjectId>, State<AppState>),
) -> impl Future<Item = Json<Project>, Error = AppError> {
    state
        .executor
        .send(ListProject { id: *id })
        .from_err()
        .and_then(|res| res.map(|x| Json(x.into())))
        .responder()
}

pub fn create(
    db_path: &str,
) -> Result<HttpServer<App<AppState>, impl Fn() -> App<AppState> + Clone>, Error> {
    let manager = ConnectionManager::<SqliteConnection>::new(db_path);
    let pool = Pool::builder().build(manager)?;
    let executor = SyncArbiter::start(3, move || Executor { db: pool.clone() });

    Ok(server::new(move || {
        App::with_state(AppState {
            executor: executor.clone(),
        })
        .middleware(Logger::default())
        .resource("/projects/create", |r| {
            r.method(Method::POST).with_async(create_project)
        })
        .resource("/projects/{id}", |r| {
            r.method(Method::GET).with_async(list_project)
        })
    }))
}

// Failure usage: https://github.com/rust-console/cargo-n64/blob/a4c93f9bb145f3ee8ac6d09e05e8ff4554b68a2d/src/lib.rs#L108-L137

#[cfg(test)]
mod test {
    use std::fs;
    use std::path::Path;
    use std::sync::mpsc;

    use actix::{Actor, Addr, Handler, Message, SyncArbiter, SyncContext};
    use actix_web::http::{Method, StatusCode};
    use actix_web::test::TestServer;
    use actix_web::HttpResponse;
    use diesel::prelude::*;
    use diesel::r2d2::{ConnectionManager, Pool};
    use diesel::sqlite::SqliteConnection;
    use failure::Error;
    use tempdir::TempDir;

    use crate::database::models::{Event, NewEvent};
    use crate::database::schema;
    use crate::database::schema::events::dsl::*;

    use super::{create_project, AppState, CreateProject, Executor};

    #[test]
    fn test_create_project() -> Result<(), Error> {
        let tmpdir = TempDir::new("db")?;

        let db_path = tmpdir.path().join("db.sqlite");
        let manager = ConnectionManager::<SqliteConnection>::new(db_path.to_str().unwrap());
        let pool = Pool::builder().build(manager)?;
        let db = pool.get()?;
        diesel_migrations::run_pending_migrations(&db)?;
        
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let sys = actix::System::new("test-feature-toggler");
            let server = super::create(db_path.clone().to_str().unwrap()).unwrap();
            server.bind("127.0.0.1:8088").unwrap().start();
            tx.send("127.0.0.1:8088").unwrap();
            let _ = sys.run();
        });

        let addr = rx.recv()?;

        let client = reqwest::Client::new();
        let response = client
            .post(&format!("http://{}/projects/create", addr))
            .json(&CreateProject {
                name: "test".to_owned(),
            })
            .send()?;

        assert_eq!(response.status(), reqwest::StatusCode::OK);

        Ok(())
    }

    #[test]
    fn test_list_project() -> Result<(), Error> {
        let tmpdir = TempDir::new("db")?;

        let db_path = tmpdir.path().join("db.sqlite");
        let manager = ConnectionManager::<SqliteConnection>::new(db_path.to_str().unwrap());
        let pool = Pool::builder().build(manager)?;
        let db = pool.get()?;
        diesel_migrations::run_pending_migrations(&db)?;

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
            .execute(&db)?;
        
        let (tx, rx) = mpsc::channel();

        std::thread::spawn(move || {
            let sys = actix::System::new("test-feature-toggler");
            let server = super::create(db_path.clone().to_str().unwrap()).unwrap();
            server.bind("127.0.0.1:8089").unwrap().start();
            tx.send("127.0.0.1:8089").unwrap();
            let _ = sys.run();
        });

        let addr = rx.recv()?;

        let client = reqwest::Client::new();
        let response = client
            .get(&format!("http://{}/projects/936da01f-9abd-4d9d-80c7-02af85c822a8", addr))
            .send()?;

        assert_eq!(response.status(), reqwest::StatusCode::OK);

        Ok(())
    }
}
