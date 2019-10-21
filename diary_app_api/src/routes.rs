use actix_web::http::StatusCode;
use actix_web::web::{Data, Json, Query};
use actix_web::HttpResponse;
use chrono::{DateTime, Local, NaiveDate, Utc};
use failure::Error;
use futures::Future;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::app::AppState;
use super::logged_user::LoggedUser;
use super::requests::{DiaryAppRequests, ListOptions, SearchOptions};

fn form_http_response(body: String) -> Result<HttpResponse, Error> {
    Ok(HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body))
}

fn to_json<T>(js: &T) -> Result<HttpResponse, Error>
where
    T: Serialize,
{
    Ok(HttpResponse::Ok().json2(js))
}

fn _search(
    query: SearchOptions,
    state: Data<AppState>,
    is_api: bool,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let req = DiaryAppRequests::Search(query);

    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            if is_api {
                let body = hashmap! {"text" => body.join("\n")};
                to_json(&body)
            } else {
                let body = format!(
                    r#"<textarea autofocus readonly="readonly" rows=50 cols=100>{}</textarea>"#,
                    body.join("\n")
                );
                form_http_response(body)
            }
        })
    })
}

pub fn search_api(
    query: Query<SearchOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    _search(query.into_inner(), state, true)
}

pub fn search(
    query: Query<SearchOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    _search(query.into_inner(), state, false)
}

#[derive(Serialize, Deserialize)]
pub struct InsertData {
    pub text: String,
}

pub fn insert(
    data: Json<InsertData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let req = DiaryAppRequests::Insert(data.into_inner().text);
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            let body = hashmap! {"datetime" => body.join("\n")};
            to_json(&body)
        })
    })
}

pub fn _sync(
    state: Data<AppState>,
    is_api: bool,
) -> impl Future<Item = HttpResponse, Error = Error> {
    state
        .db
        .send(DiaryAppRequests::Sync)
        .from_err()
        .and_then(move |res| {
            res.and_then(|body| {
                if is_api {
                    let body = hashmap! {"response" => body.join("\n")};
                    to_json(&body)
                } else {
                    let body = format!(
                        r#"<textarea autofocus readonly="readonly" rows=50 cols=100>{}</textarea>"#,
                        body.join("\n")
                    );
                    form_http_response(body)
                }
            })
        })
}

pub fn sync(
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    _sync(state, false)
}

pub fn sync_api(
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    _sync(state, true)
}

#[derive(Serialize, Deserialize)]
pub struct ReplaceData {
    pub date: NaiveDate,
    pub text: String,
}

pub fn replace(
    data: Json<ReplaceData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let data = data.into_inner();
    let req = DiaryAppRequests::Replace {
        date: data.date,
        text: data.text,
    };
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            let body = hashmap! {"entry" => body.join("\n")};
            to_json(&body)
        })
    })
}

fn _list_string(conflicts: HashSet<String>, body: Vec<String>, query: ListOptions) -> String {
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

fn _list(
    query: ListOptions,
    state: Data<AppState>,
    is_api: bool,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let req = DiaryAppRequests::List(query);
    state
        .db
        .send(req)
        .from_err()
        .join(
            state
                .db
                .send(DiaryAppRequests::ListConflicts(None))
                .from_err(),
        )
        .and_then(move |(res0, res1)| {
            res0.and_then(|body| {
                if is_api {
                    let body = hashmap! {"list" => body };
                    to_json(&body)
                } else {
                    let conflicts: HashSet<_> = res1?.into_iter().collect();
                    let body = _list_string(conflicts, body, query);
                    form_http_response(body)
                }
            })
        })
}

pub fn list(
    query: Query<ListOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    _list(query.into_inner(), state, false)
}

pub fn list_api(
    query: Query<ListOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    _list(query.into_inner(), state, true)
}

#[derive(Serialize, Deserialize)]
pub struct EditData {
    pub date: NaiveDate,
}

pub fn edit(
    query: Query<EditData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let diary_date = query.into_inner().date;
    let req = DiaryAppRequests::Display(diary_date);
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|text| {
            let body = include_str!("../../templates/editor_template.html")
                .replace("CURRENT_DIARY_TEXT", &text.join("\n"))
                .replace("DIARY_DATE", &diary_date.to_string());
            form_http_response(body)
        })
    })
}

pub fn display(
    query: Query<EditData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let diary_date = query.into_inner().date;
    let req = DiaryAppRequests::Display(diary_date);
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|text| {
            let body = format!(
                r#"<textarea autofocus readonly="readonly" rows=50 cols=100>{}</textarea><br>{}"#,
                text.join("\n"),
                format!(r#"<input type="button" name="edit" value="Edit" onclick="switchToEditor('{}')">"#, diary_date),
            );
            form_http_response(body)
        })
    })
}

pub fn diary_frontpage(
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let query = ListOptions {
        limit: Some(10),
        ..Default::default()
    };
    let req = DiaryAppRequests::List(query);
    state
        .db
        .send(req)
        .from_err()
        .join(
            state
                .db
                .send(DiaryAppRequests::ListConflicts(None))
                .from_err(),
        )
        .and_then(move |(res0, res1)| {
            res0.and_then(|body| {
                let conflicts: HashSet<_> = res1?.into_iter().collect();
                let body = _list_string(conflicts, body, query);
                let body = include_str!("../../templates/list_template.html")
                    .replace("LIST_TEXT", &body)
                    .replace("DISPLAY_TEXT", "");
                form_http_response(body)
            })
        })
}

#[derive(Serialize, Deserialize)]
pub struct ConflictData {
    pub date: Option<NaiveDate>,
    pub datetime: Option<DateTime<Utc>>,
}

pub fn list_conflicts(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let diary_date = query.into_inner().date;
    let req = DiaryAppRequests::ListConflicts(diary_date);
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
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
                    <input type="button"
                        type="submit"
                        name="remove_{t}"
                        value="Remove {t}"
                        onclick="removeConflict( '{t}' )">
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
        })
    })
}

pub fn show_conflict(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let query = query.into_inner();
    let datetime = query.datetime.unwrap_or_else(Utc::now);
    let diary_date = query
        .date
        .unwrap_or_else(|| datetime.with_timezone(&Local).naive_local().date());
    let req = DiaryAppRequests::ShowConflict(datetime);
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|text| {
            let body = format!(
                r#"{t}<br>
                    <input type="button" name="edit" value="Display" onclick="switchToDisplay('{d}')">
                    <input type="button" name="edit" value="Edit" onclick="switchToEditor('{d}')">
                    "#,
                t = text.join("\n"),
                d = diary_date,
            );
            form_http_response(body)
        })
    })
}

pub fn remove_conflict(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let datetime = query.into_inner().datetime.unwrap_or_else(Utc::now);
    let req = DiaryAppRequests::RemoveConflict(datetime);
    state
        .db
        .send(req)
        .from_err()
        .and_then(move |res| res.and_then(|text| form_http_response(text.join("\n"))))
}
