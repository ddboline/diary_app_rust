use chrono::{DateTime, Local, NaiveDate, Utc};
use handlebars::Handlebars;
use itertools::Itertools;
use lazy_static::lazy_static;
use log::debug;
use maplit::hashmap;
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashSet;
use warp::{Rejection, Reply};

use super::{
    app::AppState,
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    requests::{DiaryAppRequests, ListOptions, SearchOptions},
};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

lazy_static! {
    static ref HANDLEBARS: Handlebars<'static> = {
        let mut h = Handlebars::new();
        h.register_template_string("id", include_str!("../../templates/index.html.hbr"))
            .expect("Failed to parse template");
        h
    };
}

pub async fn search_api(
    query: SearchOptions,
    _: LoggedUser,
    state: AppState,
) -> WarpResult<impl Reply> {
    let body = search_api_body(query, state).await?;
    let body = hashmap! {"text" => body.join("\n")};
    Ok(warp::reply::json(&body))
}

async fn search_api_body(query: SearchOptions, state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Search(query)
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

pub async fn search(
    query: SearchOptions,
    _: LoggedUser,
    state: AppState,
) -> WarpResult<impl Reply> {
    let body = search_body(query, state).await?;
    let body = format!(
        r#"<textarea autofocus readonly="readonly"
            name="message" id="diary_editor_form"
            rows=50 cols=100>{}</textarea>"#,
        body.join("\n")
    );
    Ok(warp::reply::html(body))
}

async fn search_body(query: SearchOptions, state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Search(query)
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

#[derive(Serialize, Deserialize)]
pub struct InsertData {
    pub text: StackString,
}

pub async fn insert(data: InsertData, _: LoggedUser, state: AppState) -> WarpResult<impl Reply> {
    let body = insert_body(data, state).await?;
    let body = hashmap! {"datetime" => body.join("\n")};
    Ok(warp::reply::json(&body))
}

async fn insert_body(data: InsertData, state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Insert(data.text)
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

pub async fn sync(_: LoggedUser, state: AppState) -> WarpResult<impl Reply> {
    let body = sync_body(state).await?;
    let body = format!(
        r#"<textarea autofocus readonly="readonly" name="message" id="diary_editor_form" rows=50 cols=100>{}</textarea>"#,
        body.join("\n")
    );
    Ok(warp::reply::html(body))
}

async fn sync_body(state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Sync
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

pub async fn sync_api(_: LoggedUser, state: AppState) -> WarpResult<impl Reply> {
    let body = sync_body(state).await?;
    let body = hashmap! {"response" => body.join("\n")};
    Ok(warp::reply::json(&body))
}

#[derive(Serialize, Deserialize)]
pub struct ReplaceData {
    pub date: NaiveDate,
    pub text: StackString,
}

pub async fn replace(data: ReplaceData, _: LoggedUser, state: AppState) -> WarpResult<impl Reply> {
    let body = replace_body(data, state).await?;
    let body = hashmap! {"entry" => body.join("\n")};
    Ok(warp::reply::json(&body))
}

async fn replace_body(data: ReplaceData, state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Replace {
        date: data.date,
        text: data.text,
    }
    .handle(&state.db)
    .await
    .map_err(Into::into)
}

fn _list_string<T, U>(conflicts: &HashSet<StackString>, body: T, query: ListOptions) -> String
where
    T: IntoIterator<Item = U>,
    U: AsRef<str>,
{
    let text = body
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
                t = t.as_ref(),
                c = if conflicts.contains(t.as_ref()) {
                    format!(
                        r#"
                            <input type="button"
                                type="submit"
                                name="conflict_{t}"
                                value="Conflict {t}"
                                onclick="listConflicts( '{t}' )"
                            >"#,
                        t = t.as_ref()
                    )
                } else {
                    "".to_string()
                }
            )
        })
        .join("\n");
    let buttons = if query.start.is_some() {
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
        .join("\n")
    } else {
        vec![format!(
            r#"<button type="submit" onclick="gotoEntries({})">Next</button>"#,
            10
        )]
        .join("\n")
    };
    format!("{}\n<br>\n{}", text, buttons)
}

pub async fn list(query: ListOptions, _: LoggedUser, state: AppState) -> WarpResult<impl Reply> {
    let body = list_body(query, &state).await?;
    Ok(warp::reply::html(body))
}

async fn list_body(query: ListOptions, state: &AppState) -> HttpResult<String> {
    let body = list_api_body(query, state).await?;
    let conflicts: HashSet<_> = DiaryAppRequests::ListConflicts(None)
        .handle(&state.db)
        .await?
        .into_iter()
        .collect();
    let body = _list_string(&conflicts, body, query);
    Ok(body)
}

async fn list_api_body(query: ListOptions, state: &AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::List(query)
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

pub async fn list_api(
    query: ListOptions,
    _: LoggedUser,
    state: AppState,
) -> WarpResult<impl Reply> {
    let body = list_api_body(query, &state).await?;
    let body = hashmap! {"list" => body };
    Ok(warp::reply::json(&body))
}

#[derive(Serialize, Deserialize)]
pub struct EditData {
    pub date: NaiveDate,
}

pub async fn edit(query: EditData, _: LoggedUser, state: AppState) -> WarpResult<impl Reply> {
    let body = edit_body(query, state).await?;
    Ok(warp::reply::html(body))
}

async fn edit_body(query: EditData, state: AppState) -> HttpResult<String> {
    let diary_date = query.date;
    let text = DiaryAppRequests::Display(diary_date)
        .handle(&state.db)
        .await?;
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
    Ok(body)
}

pub async fn display(query: EditData, _: LoggedUser, state: AppState) -> WarpResult<impl Reply> {
    let body = display_body(query, state).await?;
    Ok(warp::reply::html(body))
}

async fn display_body(query: EditData, state: AppState) -> HttpResult<String> {
    let diary_date = query.date;
    let text = DiaryAppRequests::Display(diary_date)
        .handle(&state.db)
        .await?;
    let body = format!(
        r#"<textarea autofocus readonly="readonly" name="message" id="diary_editor_form" rows=50 cols=100>{text}</textarea><br>{editor}"#,
        text = text.join("\n"),
        editor = format!(
            r#"<input type="button" name="edit" value="Edit" onclick="switchToEditor('{}')">"#,
            diary_date
        ),
    );
    Ok(body)
}

pub async fn diary_frontpage(_: LoggedUser, state: AppState) -> WarpResult<impl Reply> {
    let body = diary_frontpage_body(state).await?;
    Ok(warp::reply::html(body))
}

async fn diary_frontpage_body(state: AppState) -> HttpResult<String> {
    let query = ListOptions {
        limit: Some(10),
        ..ListOptions::default()
    };
    let body = DiaryAppRequests::List(query).handle(&state.db).await?;
    debug!("Got list");
    assert!(false);
    let conflicts: HashSet<_> = DiaryAppRequests::ListConflicts(None)
        .handle(&state.db)
        .await?
        .into_iter()
        .collect();
    debug!("Got conflicts");
    let body = _list_string(&conflicts, body, query);
    let params = hashmap! {
        "LIST_TEXT" => body.as_str(),
        "DISPLAY_TEXT" => "",
    };
    let body = HANDLEBARS.render("id", &params)?;
    Ok(body)
}

#[derive(Serialize, Deserialize)]
pub struct ConflictData {
    pub date: Option<NaiveDate>,
    pub datetime: Option<DateTime<Utc>>,
}

pub async fn list_conflicts(
    query: ConflictData,
    _: LoggedUser,
    state: AppState,
) -> WarpResult<impl Reply> {
    let body = list_conflicts_body(query, state).await?;
    Ok(warp::reply::html(body))
}

async fn list_conflicts_body(query: ConflictData, state: AppState) -> HttpResult<String> {
    let diary_date = query.date;
    let body = DiaryAppRequests::ListConflicts(diary_date)
        .handle(&state.db)
        .await?;
    let mut buttons = Vec::new();
    if let Some(date) = diary_date {
        if !body.is_empty() {
            buttons.push(format!(
                r#"<button type="submit" onclick="cleanConflicts('{}')">Clean</button>"#,
                date
            ))
        }
    }
    buttons.push(r#"<button type="submit" onclick="switchToList()">List</button>"#.to_string());

    let text = body
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
        .join("\n");

    let body = format!("{}\n<br>\n{}", text, buttons.join("<br>"));
    Ok(body)
}

pub async fn show_conflict(
    query: ConflictData,
    _: LoggedUser,
    state: AppState,
) -> WarpResult<impl Reply> {
    let body = show_conflict_body(query, state).await?;
    Ok(warp::reply::html(body))
}

async fn show_conflict_body(query: ConflictData, state: AppState) -> HttpResult<String> {
    let datetime = query.datetime.unwrap_or_else(Utc::now);
    let diary_date = query
        .date
        .unwrap_or_else(|| datetime.with_timezone(&Local).naive_local().date());
    let text = DiaryAppRequests::ShowConflict(datetime)
        .handle(&state.db)
        .await?;
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
    Ok(body)
}

pub async fn remove_conflict(
    query: ConflictData,
    _: LoggedUser,
    state: AppState,
) -> WarpResult<impl Reply> {
    let body = remove_conflict_body(query, state).await?;
    Ok(warp::reply::html(body))
}

async fn remove_conflict_body(query: ConflictData, state: AppState) -> HttpResult<String> {
    let body = if let Some(datetime) = query.datetime {
        let text = DiaryAppRequests::RemoveConflict(datetime)
            .handle(&state.db)
            .await?;
        text.join("\n")
    } else if let Some(date) = query.date {
        let text = DiaryAppRequests::CleanConflicts(date)
            .handle(&state.db)
            .await?;
        text.join("\n")
    } else {
        "".to_string()
    };
    Ok(body)
}

#[derive(Serialize, Deserialize)]
pub struct ConflictUpdateData {
    pub id: i32,
    pub diff_type: StackString,
}

pub async fn update_conflict(
    query: ConflictUpdateData,
    _: LoggedUser,
    state: AppState,
) -> WarpResult<impl Reply> {
    update_conflict_body(query, state).await?;
    Ok(warp::reply::html("finished".to_string()))
}

async fn update_conflict_body(query: ConflictUpdateData, state: AppState) -> HttpResult<()> {
    DiaryAppRequests::UpdateConflict {
        id: query.id,
        diff_text: query.diff_type,
    }
    .handle(&state.db)
    .await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
pub struct CommitConflictData {
    pub datetime: DateTime<Utc>,
}

pub async fn commit_conflict(
    query: CommitConflictData,
    _: LoggedUser,
    state: AppState,
) -> WarpResult<impl Reply> {
    let body = commit_conflict_body(query, state).await?;
    let body = hashmap! {"entry" => body.join("\n")};
    Ok(warp::reply::json(&body))
}

async fn commit_conflict_body(
    query: CommitConflictData,
    state: AppState,
) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::CommitConflict(query.datetime)
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

pub async fn user(user: LoggedUser) -> WarpResult<impl Reply> {
    Ok(warp::reply::json(&user))
}
