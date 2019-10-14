use actix_web::http::StatusCode;
use actix_web::web::{Data, Json, Query};
use actix_web::HttpResponse;
use chrono::NaiveDate;
use failure::Error;
use futures::Future;
use maplit::hashmap;
use serde::{Deserialize, Serialize};

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
    query: Query<SearchOptions>,
    state: Data<AppState>,
    do_api: bool,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let req = DiaryAppRequests::Search(query.into_inner());

    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            if do_api {
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
    _search(query, state, true)
}

pub fn search(
    query: Query<SearchOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    _search(query, state, false)
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
    let text = data.into_inner().text;
    let req = DiaryAppRequests::Insert(text);
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            let body = hashmap! {"datetime" => body.join("\n")};
            to_json(&body)
        })
    })
}

pub fn sync(
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    state
        .db
        .send(DiaryAppRequests::Sync)
        .from_err()
        .and_then(move |res| {
            res.and_then(|body| {
                let body = hashmap! {"response" => body.join("\n")};
                to_json(&body)
            })
        })
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

pub fn list(
    query: Query<ListOptions>,
    _: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let query = query.into_inner();
    let req = DiaryAppRequests::List(query);
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            let body = hashmap! {"list" => body };
            to_json(&body)
        })
    })
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
    let req = DiaryAppRequests::Search(SearchOptions {
        date: Some(diary_date),
        text: None,
    });
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|text| {
            let body = include_str!("../../templates/editor_template.html")
                .replace("DIARY_DATE", &diary_date.to_string())
                .replace("CURRENT_DIARY_TEXT", &text.join("\n"));
            form_http_response(body)
        })
    })
}
