// Need a macro_use so that macros are brought
// in globally for use in crate::database::schema
#[macro_use]
extern crate diesel;

mod app;
mod database;
mod domain;
mod project;
mod toggle;

use actix::SyncArbiter;
use actix_web::middleware::Logger;
use actix_web::{http::Method, server, App};
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use failure::Error;

use crate::app::{AppState, Executor};

fn main() -> Result<(), Error> {
    std::env::set_var("RUST_LOG", "actix_web=debug");
    env_logger::init();

    let sys = actix::System::new("feature-toggler");

    app::create("db.sqlite")?.bind("127.0.0.1:8088")?.start();

    let _ = sys.run();

    Ok(())
}
