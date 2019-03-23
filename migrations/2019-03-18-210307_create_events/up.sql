CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    aggregate_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    type TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE INDEX ix_events_aggregate_id ON events (aggregate_id);
CREATE UNIQUE INDEX uq_aggregate_id_generation_id ON events (aggregate_id, generation);
