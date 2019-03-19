use diesel::{Insertable, Queryable};

use super::schema::events;

#[derive(Clone, Debug, Eq, PartialEq, Queryable)]
pub struct Event {
    pub id: String,
    pub aggregate_id: String,
    pub created_at: String,
    pub type_: String,
    pub data: String,
}

#[derive(Debug, Insertable)]
#[table_name="events"]
pub struct NewEvent<'a> {
    pub id: &'a str,
    pub aggregate_id: &'a str,
    pub created_at: &'a str,
    pub type_: &'a str,
    pub data: &'a str,
}
