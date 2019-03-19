table! {
    events (id) {
        id -> Text,
        aggregate_id -> Text,
        created_at -> Text,
        #[sql_name = "type"]
        type_ -> Text,
        data -> Text,
    }
}
