use actix_web::{
    http::StatusCode,
    web::{Data, Json, Query},
    HttpResponse,
};
use chrono::{DateTime, Local, NaiveDate, Utc};
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::{
    app::AppState,
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    requests::{DiaryAppRequests, HandleRequest, ListOptions, SearchOptions},
};

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

fn to_json<T>(js: T) -> Result<HttpResponse, Error>
where
    T: Serialize,
{
    Ok(HttpResponse::Ok().json(js))
}

async fn _search(
    query: SearchOptions,
    state: Data<AppState>,
    is_api: bool,
) -> Result<HttpResponse, Error> {
    let req = DiaryAppRequests::Search(query);

    let body = state.db.handle(req).await?;

    if is_api {
        let body = hashmap! {"text" => body.join("\n")};
        to_json(body)
    } else {
        let body = format!(
            r#"<textarea autofocus readonly="readonly"
                name="message" id="diary_editor_form"
                rows=50 cols=100>{}</textarea>"#,
            body.join("\n")
        );
        form_http_response(body)
    }
}

pub async fn search_api(
    query: Query<SearchOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    _search(query.into_inner(), state, true).await
}

pub async fn search(
    query: Query<SearchOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    _search(query.into_inner(), state, false).await
}

#[derive(Serialize, Deserialize)]
pub struct InsertData {
    pub text: String,
}

pub async fn insert(
    data: Json<InsertData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let req = DiaryAppRequests::Insert(data.into_inner().text);

    let body = state.db.handle(req).await?;
    let body = hashmap! {"datetime" => body.join("\n")};
    to_json(body)
}

pub async fn _sync(state: Data<AppState>, is_api: bool) -> Result<HttpResponse, Error> {
    let body = state.db.handle(DiaryAppRequests::Sync).await?;
    if is_api {
        let body = hashmap! {"response" => body.join("\n")};
        to_json(body)
    } else {
        let body = format!(
            r#"<textarea autofocus readonly="readonly" name="message" id="diary_editor_form" rows=50 cols=100>{}</textarea>"#,
            body.join("\n")
        );
        form_http_response(body)
    }
}

pub async fn sync(_: LoggedUser, state: Data<AppState>) -> Result<HttpResponse, Error> {
    _sync(state, false).await
}

pub async fn sync_api(_: LoggedUser, state: Data<AppState>) -> Result<HttpResponse, Error> {
    _sync(state, true).await
}

#[derive(Serialize, Deserialize)]
pub struct ReplaceData {
    pub date: NaiveDate,
    pub text: String,
}

pub async fn replace(
    data: Json<ReplaceData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let data = data.into_inner();
    let req = DiaryAppRequests::Replace {
        date: data.date,
        text: data.text,
    };
    let body = state.db.handle(req).await?;
    let body = hashmap! {"entry" => body.join("\n")};
    to_json(body)
}

fn _list_string(conflicts: &HashSet<String>, body: Vec<String>, query: ListOptions) -> String {
    let text: Vec<_> = body
        .into_iter()
        .map(|t| {
            format!(
                r#"
                    <input type="button"
                        type="submit"
                        name="{t}"
                        value="{t}"
                        onclick="switchToDate( '{t}' )">{c}
                    <br>"#,
                t = t,
                c = if conflicts.contains(&t) {
                    format!(
                        r#"
                            <input type="button"
                                type="submit"
                                name="conflict_{t}"
                                value="Conflict {t}"
                                onclick="listConflicts( '{t}' )"
                            >"#,
                        t = t
                    )
                } else {
                    "".to_string()
                }
            )
        })
        .collect();
    let buttons: Vec<_> = if query.start.is_some() {
        vec![
            format!(
                r#"<button type="submit" onclick="gotoEntries({})">Previous</button>"#,
                -10
            ),
            format!(
                r#"<button type="submit" onclick="gotoEntries({})">Next</button>"#,
                10
            ),
        ]
    } else {
        vec![format!(
            r#"<button type="submit" onclick="gotoEntries({})">Next</button>"#,
            10
        )]
    };
    format!("{}\n<br>\n{}", text.join("\n"), buttons.join("\n"))
}

async fn _list(
    query: ListOptions,
    state: Data<AppState>,
    is_api: bool,
) -> Result<HttpResponse, Error> {
    let req = DiaryAppRequests::List(query);
    let body = state.db.handle(req).await?;

    if is_api {
        let body = hashmap! {"list" => body };
        to_json(body)
    } else {
        let conflicts: Vec<String> = state
            .db
            .handle(DiaryAppRequests::ListConflicts(None))
            .await?;
        let conflicts: HashSet<String> = conflicts.into_iter().collect();
        let body = _list_string(&conflicts, body, query);
        form_http_response(body)
    }
}

pub async fn list(
    query: Query<ListOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    _list(query.into_inner(), state, false).await
}

pub async fn list_api(
    query: Query<ListOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    _list(query.into_inner(), state, true).await
}

#[derive(Serialize, Deserialize)]
pub struct EditData {
    pub date: NaiveDate,
}

pub async fn edit(
    query: Query<EditData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let diary_date = query.date;
    let req = DiaryAppRequests::Display(diary_date);

    let text = state.db.handle(req).await?;
    let body = format!(
        r#"
        <textarea name="message" id="diary_editor_form" rows=50 cols=100
        form="diary_edit_form">{text}</textarea><br>
        <form id="diary_edit_form">
        <input type="button" name="update" value="Update" onclick="submitFormData('{date}')">
        <input type="button" name="cancel" value="Cancel" onclick="switchToDisplay('{date}')">
        </form>"#,
        text = text.join("\n"),
        date = diary_date,
    );

    form_http_response(body)
}

pub async fn display(
    query: Query<EditData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let diary_date = query.date;
    let req = DiaryAppRequests::Display(diary_date);
    let text = state.db.handle(req).await?;
    let body = format!(
        r#"<textarea autofocus readonly="readonly" name="message" id="diary_editor_form" rows=50 cols=100>{text}</textarea><br>{editor}"#,
        text = text.join("\n"),
        editor = format!(
            r#"<input type="button" name="edit" value="Edit" onclick="switchToEditor('{}')">"#,
            diary_date
        ),
    );
    form_http_response(body)
}

pub async fn diary_frontpage(_: LoggedUser, state: Data<AppState>) -> Result<HttpResponse, Error> {
    let query = ListOptions {
        limit: Some(10),
        ..ListOptions::default()
    };
    let req = DiaryAppRequests::List(query);
    let body = state.db.handle(req).await?;

    let conflicts: HashSet<_> = state
        .db
        .handle(DiaryAppRequests::ListConflicts(None))
        .await?
        .into_iter()
        .collect();
    let body = _list_string(&conflicts, body, query);
    let body = include_str!("../../templates/index.html")
        .replace("LIST_TEXT", &body)
        .replace("DISPLAY_TEXT", "");
    form_http_response(body)
}

#[derive(Serialize, Deserialize)]
pub struct ConflictData {
    pub date: Option<NaiveDate>,
    pub datetime: Option<DateTime<Utc>>,
}

pub async fn list_conflicts(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let diary_date = query.into_inner().date;
    let req = DiaryAppRequests::ListConflicts(diary_date);

    let body = state.db.handle(req).await?;
    let text: Vec<_> = body
        .into_iter()
        .map(|t| {
            format!(
                r#"
            <input type="button"
                type="submit"
                name="show_{t}"
                value="Show {t}"
                onclick="showConflict( '{d}', '{t}' )">
            <br>
        "#,
                t = t,
                d = diary_date
                    .unwrap_or_else(|| Local::today().naive_local())
                    .to_string(),
            )
        })
        .collect();
    let button = r#"<button type="submit" onclick="switchToList()">List</button>"#;
    let body = format!("{}\n<br>\n{}", text.join("\n"), button);
    form_http_response(body)
}

pub async fn show_conflict(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let datetime = query.datetime.unwrap_or_else(Utc::now);
    let diary_date = query
        .date
        .unwrap_or_else(|| datetime.with_timezone(&Local).naive_local().date());
    let req = DiaryAppRequests::ShowConflict(datetime);

    let text = state.db.handle(req).await?;
    let body = format!(
        r#"{t}<br>
            <input type="button" name="display" value="Display" onclick="switchToDisplay('{d}')">
            <input type="button" name="commit" value="Commit" onclick="commitConflict('{d}', '{dt}')">
            <input type="button" name="remove" value="Remove" onclick="removeConflict('{d}', '{dt}')">
            <input type="button" name="edit" value="Edit" onclick="switchToEditor('{d}')">
            "#,
        t = text.join("\n"),
        d = diary_date,
        dt = datetime.format("%Y-%m-%dT%H:%M:%S%.fZ"),
    );
    form_http_response(body)
}

pub async fn remove_conflict(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let datetime = query.into_inner().datetime.unwrap_or_else(Utc::now);
    let req = DiaryAppRequests::RemoveConflict(datetime);

    let text = state.db.handle(req).await?;
    form_http_response(text.join("\n"))
}

#[derive(Serialize, Deserialize)]
pub struct ConflictUpdateData {
    pub id: i32,
    pub diff_type: String,
}

pub async fn update_conflict(
    query: Query<ConflictUpdateData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let req = DiaryAppRequests::UpdateConflict {
        id: query.id,
        diff_text: query.diff_type,
    };

    state.db.handle(req).await?;

    form_http_response("finished".to_string())
}

#[derive(Serialize, Deserialize)]
pub struct CommitConflictData {
    pub datetime: DateTime<Utc>,
}

pub async fn commit_conflict(
    query: Query<CommitConflictData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> Result<HttpResponse, Error> {
    let query = query.into_inner();
    let req = DiaryAppRequests::CommitConflict(query.datetime);

    let body = state.db.handle(req).await?;
    let body = hashmap! {"entry" => body.join("\n")};
    to_json(body)
}

pub async fn user(user: LoggedUser) -> Result<HttpResponse, Error> {
    to_json(user)
}
