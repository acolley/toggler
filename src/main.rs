// Need a macro_use so that macros are brought
// in globally for use in crate::database::schema
#[macro_use]
extern crate diesel;

mod database;
mod domain;
mod project;

use actix::{Actor, Addr, Handler, Message, SyncArbiter, SyncContext};
use actix_web::middleware::Logger;
use actix_web::AsyncResponder;
use actix_web::{
    dev::FromParam, error::ResponseError, http::Method, http::StatusCode, server, App,
    HttpResponse, Json, Path, State,
};
use chrono::Utc;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use diesel::Connection;
use failure::Error;
use failure_derive::Fail;
use futures::Future;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    db: Pool<ConnectionManager<SqliteConnection>>,
}

impl Actor for Executor {
    type Context = SyncContext<Self>;
}

#[derive(Clone)]
pub struct AppState {
    executor: Addr<Executor>,
}

#[derive(Debug, Deserialize, Serialize)]
struct CreateProject {
    name: String,
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
fn create_project(
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

/// Domain Project to DAO Project
impl From<project::Project> for Project {
    fn from(p: project::Project) -> Self {
        Self {
            id: p.id.into(),
            name: p.name,
        }
    }
}

fn list_project(
    (id, state): (Path<ProjectId>, State<AppState>),
) -> impl Future<Item = Json<Project>, Error = AppError> {
    state
        .executor
        .send(ListProject { id: *id })
        .from_err()
        .and_then(|res| res.map(|x| Json(x.into())))
        .responder()
}

// Failure usage: https://github.com/rust-console/cargo-n64/blob/a4c93f9bb145f3ee8ac6d09e05e8ff4554b68a2d/src/lib.rs#L108-L137

fn main() -> Result<(), Error> {
    std::env::set_var("RUST_LOG", "actix_web=debug");
    env_logger::init();

    let sys = actix::System::new("feature-toggler");

    let manager = ConnectionManager::<SqliteConnection>::new("db.sqlite");
    let pool = Pool::builder().build(manager)?;
    let executor = SyncArbiter::start(3, move || Executor { db: pool.clone() });

    server::new(move || {
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
    })
    .bind("127.0.0.1:8088")?
    .start();

    let _ = sys.run();

    Ok(())
}
