use actix_web::http::StatusCode;
use actix_web::web::{Data, Json, Query};
use actix_web::HttpResponse;
use chrono::NaiveDate;
use failure::Error;
use futures::Future;
use serde::{Deserialize, Serialize};

use super::app::AppState;
use super::logged_user::LoggedUser;
use super::requests::{DiaryAppRequests, SearchOptions};

fn form_http_response(body: String) -> HttpResponse {
    HttpResponse::build(StatusCode::OK)
        .content_type("text/html; charset=utf-8")
        .body(body)
}

pub fn search(
    query: Query<SearchOptions>,
    user: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let req = DiaryAppRequests::Search(query.into_inner());

    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            if !state.user_list.is_authorized(&user) {
                return Ok(HttpResponse::Unauthorized()
                    .json(format!("Unauthorized {:?}", state.user_list)));
            }
            Ok(form_http_response(body))
        })
    })
}

#[derive(Serialize, Deserialize)]
pub struct InsertData {
    pub text: String,
}

pub fn insert(
    data: Json<InsertData>,
    user: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let text = data.into_inner().text;
    let req = DiaryAppRequests::Insert(text);
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            if !state.user_list.is_authorized(&user) {
                return Ok(HttpResponse::Unauthorized()
                    .json(format!("Unauthorized {:?}", state.user_list)));
            }
            Ok(form_http_response(body))
        })
    })
}

pub fn sync(
    user: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    state
        .db
        .send(DiaryAppRequests::Sync)
        .from_err()
        .and_then(move |res| {
            res.and_then(|body| {
                if !state.user_list.is_authorized(&user) {
                    return Ok(HttpResponse::Unauthorized()
                        .json(format!("Unauthorized {:?}", state.user_list)));
                }
                Ok(form_http_response(body))
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
    user: LoggedUser,
    state: Data<AppState>,
) -> impl Future<Item = HttpResponse, Error = Error> {
    let data = data.into_inner();
    let req = DiaryAppRequests::Replace {
        date: data.date,
        text: data.text,
    };
    state.db.send(req).from_err().and_then(move |res| {
        res.and_then(|body| {
            if !state.user_list.is_authorized(&user) {
                return Ok(HttpResponse::Unauthorized()
                    .json(format!("Unauthorized {:?}", state.user_list)));
            }
            Ok(form_http_response(body))
        })
    })
}
