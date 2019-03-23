// Need a macro_use so that macros are brought
// in globally for use in crate::database::schema
#[macro_use]
extern crate diesel;

mod database;
mod project;

use actix_web::middleware::Logger;
use actix_web::{
    dev::FromParam, error::ResponseError, http::Method, http::StatusCode, server, App, HttpRequest,
    HttpResponse, Json,
};
use chrono::Utc;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use failure::Error;
use failure_derive::Fail;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::project::{
    CreateProject, CreateProjectHandler, CreateProjectHandlerError, ListProject,
    ListProjectHandler, ListProjectHandlerError, ProjectId, ProjectIdParseError, SqliteRepository, SqliteRepositoryError,
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
            AppError::CreateProjectError(_) => HttpResponse::new(StatusCode::BAD_REQUEST),
            AppError::ListProjectError(
                ListProjectHandlerError::RepositoryError(
                    SqliteRepositoryError::NotFoundError
                )
            ) => HttpResponse::new(StatusCode::NOT_FOUND),
            AppError::ListProjectError(_) => HttpResponse::new(StatusCode::INTERNAL_SERVER_ERROR),
        }
    }
}

#[derive(Clone)]
pub struct State {
    db: Pool<ConnectionManager<SqliteConnection>>,
}

fn create_project(req: &HttpRequest<State>) -> actix_web::Result<Json<Project>> {
    // TODO: run in transaction
    // TODO: return JSON response with created Project
    let db = &req.state().db.get().map_err(|e| -> AppError { e.into() })?;
    let repository = &mut SqliteRepository { db };
    let handler = &mut CreateProjectHandler {
        repository,
        utc_now: Utc::now,
    };

    let project = handler
        .handle(CreateProject {
            id: Uuid::new_v4(),
            name: "hello".to_owned(),
        })
        .map_err(|e| -> AppError { e.into() })?;

    Ok(Json(project.into()))
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

fn list_project(req: &HttpRequest<State>) -> actix_web::Result<Json<Project>> {
    let db = &req.state().db.get().map_err(|e| -> AppError { e.into() })?;
    let repository = &SqliteRepository { db };
    let handler = &ListProjectHandler { repository };

    let id: ProjectId = req.match_info().query("id")?;
    let project = handler
        .handle(ListProject { id })
        .map_err(|e| -> AppError { e.into() })?;
    Ok(Json(project.into()))
}

// Failure usage: https://github.com/rust-console/cargo-n64/blob/a4c93f9bb145f3ee8ac6d09e05e8ff4554b68a2d/src/lib.rs#L108-L137

fn main() -> Result<(), Error> {
    std::env::set_var("RUST_LOG", "actix_web=debug");
    env_logger::init();
    let manager = ConnectionManager::<SqliteConnection>::new("db.sqlite");
    let pool = Pool::builder().build(manager)?;
    server::new(move || {
        App::with_state(State { db: pool.clone() })
            .middleware(Logger::default())
            .resource("/projects/create", |r| {
                r.method(Method::POST).f(create_project)
            })
            .resource("/projects/{id}", |r| r.method(Method::GET).f(list_project))
    })
    .bind("127.0.0.1:8088")?
    .run();

    Ok(())
}
