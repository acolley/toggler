use failure_derive::Fail;

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

#[derive(Debug, Eq, Fail, PartialEq)]
pub enum ProjectError {
    #[fail(display = "invalid project name: {}", name)]
    InvalidName { name: String },
    #[fail(display = "invalid event `{}` applied to state `{}", event, state)]
    InvalidStateEvent { state: String, event: String },
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