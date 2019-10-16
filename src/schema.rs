table! {
    authorized_users (email) {
        email -> Varchar,
        telegram_userid -> Nullable<Int8>,
    }
}

table! {
    diary_cache (diary_datetime) {
        diary_datetime -> Timestamptz,
        diary_text -> Text,
    }
}

table! {
    diary_conflict (id) {
        id -> Int4,
        sync_datetime -> Timestamptz,
        diary_date -> Date,
        diff_type -> Text,
        diff_text -> Text,
    }
}

table! {
    diary_entries (diary_date) {
        diary_date -> Date,
        diary_text -> Text,
        last_modified -> Timestamptz,
    }
}

allow_tables_to_appear_in_same_query!(
    authorized_users,
    diary_cache,
    diary_conflict,
    diary_entries,
);
