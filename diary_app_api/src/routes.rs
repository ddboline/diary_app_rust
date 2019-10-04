use actix_web::http::StatusCode;
use actix_web::web::{Data, Query};
use actix_web::HttpResponse;
use failure::Error;
use futures::Future;

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
