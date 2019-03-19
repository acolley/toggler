// Need a macro_use so that macros are brought
// in globally for use in crate::database::schema
#[macro_use]
extern crate diesel;

mod database;
mod project;

use chrono::Utc;
use diesel::Connection;
use diesel::sqlite::SqliteConnection;
use failure::Error;
use uuid::Uuid;

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

fn main() -> Result<(), Error> {
    let db = &SqliteConnection::establish("db.sqlite")?;
    let repository = &mut project::SqliteRepository { db };
    let handler = &mut project::CreateProjectHandler {
        repository,
        utc_now: Utc::now,
    };

    handler.handle(&project::CreateProject {
        name: "hello".to_owned(),
    })?;

    Ok(())
}
