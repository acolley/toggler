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
use actix_web::{
    http::Method, server, App,
};
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use failure::Error;

use crate::app::{AppState, Executor};

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
            r.method(Method::POST).with_async(app::create_project)
        })
        .resource("/projects/{id}", |r| {
            r.method(Method::GET).with_async(app::list_project)
        })
    })
    .bind("127.0.0.1:8088")?
    .start();

    let _ = sys.run();

    Ok(())
}
