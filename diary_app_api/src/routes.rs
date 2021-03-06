use chrono::{Local, Utc};
use itertools::Itertools;
use log::debug;
use maplit::hashmap;
use rweb::{get, post, Json, Query, Rejection, Schema};
use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, RwebResponse,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashSet;

use diary_app_lib::{datetime_wrapper::DateTimeWrapper, naivedate_wrapper::NaiveDateWrapper};

use super::{
    app::AppState,
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    requests::{DiaryAppRequests, ListOptions, SearchOptions},
};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

#[derive(Schema, Serialize)]
struct SearchApiOutput {
    text: String,
}

#[derive(RwebResponse)]
#[response(description = "Search Result")]
struct SearchApiResponse(JsonBase<SearchApiOutput, Error>);

#[get("/api/search_api")]
pub async fn search_api(
    query: Query<SearchOptions>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SearchApiResponse> {
    let query = query.into_inner();
    let body = search_api_body(query, state).await?;
    let text = body.join("\n");
    Ok(JsonBase::new(SearchApiOutput { text }).into())
}

async fn search_api_body(query: SearchOptions, state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Search(query)
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

#[derive(RwebResponse)]
#[response(description = "Search Output", content = "html")]
struct SearchResponse(HtmlBase<String, Error>);

#[get("/api/search")]
pub async fn search(
    query: Query<SearchOptions>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SearchResponse> {
    let query = query.into_inner();
    let body = search_body(query, state).await?;
    let body = format!(
        r#"<textarea autofocus readonly="readonly"
            name="message" id="diary_editor_form"
            rows=50 cols=100>{}</textarea>"#,
        body.join("\n")
    );
    Ok(HtmlBase::new(body).into())
}

async fn search_body(query: SearchOptions, state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Search(query)
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

#[derive(Serialize, Deserialize, Schema)]
pub struct InsertData {
    pub text: StackString,
}

#[derive(Schema, Serialize)]
struct InsertDataOutput {
    datetime: String,
}

#[derive(RwebResponse)]
#[response(description = "Insert Data Result", status = "CREATED")]
struct InsertDataResponse(JsonBase<InsertDataOutput, Error>);

#[post("/api/insert")]
pub async fn insert(
    data: Json<InsertData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<InsertDataResponse> {
    let data = data.into_inner();
    let body = insert_body(data, state).await?;
    let datetime = body.join("\n");
    Ok(JsonBase::new(InsertDataOutput { datetime }).into())
}

async fn insert_body(data: InsertData, state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Insert(data.text)
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

#[derive(RwebResponse)]
#[response(description = "Sync Output", content = "html")]
struct SyncResponse(HtmlBase<String, Error>);

#[get("/api/sync")]
pub async fn sync(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SyncResponse> {
    let body = sync_body(state).await?;
    let body = format!(
        r#"<textarea autofocus readonly="readonly" name="message" id="diary_editor_form" rows=50 cols=100>{}</textarea>"#,
        body.join("\n")
    );
    Ok(HtmlBase::new(body).into())
}

async fn sync_body(state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Sync
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

#[derive(Schema, Serialize)]
struct SyncApiOutput {
    response: String,
}

#[derive(RwebResponse)]
#[response(description = "Sync Api Response")]
struct SyncApiResponse(JsonBase<SyncApiOutput, Error>);

#[get("/api/sync_api")]
pub async fn sync_api(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SyncApiResponse> {
    let body = sync_body(state).await?;
    let response = body.join("\n");
    Ok(JsonBase::new(SyncApiOutput { response }).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ReplaceData {
    pub date: NaiveDateWrapper,
    pub text: StackString,
}

#[derive(Schema, Serialize)]
struct ReplaceOutput {
    entry: String,
}

#[derive(RwebResponse)]
#[response(description = "Replace Response", status = "CREATED")]
struct ReplaceResponse(JsonBase<ReplaceOutput, Error>);

#[post("/api/replace")]
pub async fn replace(
    data: Json<ReplaceData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ReplaceResponse> {
    let data = data.into_inner();
    let body = replace_body(data, state).await?;
    let entry = body.join("\n");
    Ok(JsonBase::new(ReplaceOutput { entry }).into())
}

async fn replace_body(data: ReplaceData, state: AppState) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::Replace {
        date: data.date.into(),
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

#[derive(RwebResponse)]
#[response(description = "List Output", content = "html")]
struct ListResponse(HtmlBase<String, Error>);

#[get("/api/list")]
pub async fn list(
    query: Query<ListOptions>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ListResponse> {
    let query = query.into_inner();
    let body = list_body(query, &state).await?;
    Ok(HtmlBase::new(body).into())
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

#[derive(Schema, Serialize)]
struct ListOutput {
    list: Vec<StackString>,
}

#[derive(RwebResponse)]
#[response(description = "ListApi Response")]
struct ListApiResponse(JsonBase<ListOutput, Error>);

#[get("/api/list_api")]
pub async fn list_api(
    query: Query<ListOptions>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ListApiResponse> {
    let query = query.into_inner();
    let list = list_api_body(query, &state).await?;
    Ok(JsonBase::new(ListOutput { list }).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct EditData {
    pub date: NaiveDateWrapper,
}

#[derive(RwebResponse)]
#[response(description = "Edit Output", content = "html")]
struct EditResponse(HtmlBase<String, Error>);

#[get("/api/edit")]
pub async fn edit(
    query: Query<EditData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<EditResponse> {
    let query = query.into_inner();
    let body = edit_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn edit_body(query: EditData, state: AppState) -> HttpResult<String> {
    let diary_date = query.date.into();
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

#[derive(RwebResponse)]
#[response(description = "Display Output", content = "html")]
struct DisplayResponse(HtmlBase<String, Error>);

#[get("/api/display")]
pub async fn display(
    query: Query<EditData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<DisplayResponse> {
    let query = query.into_inner();
    let body = display_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn display_body(query: EditData, state: AppState) -> HttpResult<String> {
    let diary_date = query.date.into();
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

#[derive(RwebResponse)]
#[response(description = "Frontpage", content = "html")]
struct FrontpageResponse(HtmlBase<String, Error>);

#[get("/api/index.html")]
pub async fn diary_frontpage(
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<FrontpageResponse> {
    let body = diary_frontpage_body(state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn diary_frontpage_body(state: AppState) -> HttpResult<String> {
    let query = ListOptions {
        limit: Some(10),
        ..ListOptions::default()
    };
    let body = DiaryAppRequests::List(query).handle(&state.db).await?;
    debug!("Got list");
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
    let body = state.hb.render("id", &params)?;
    Ok(body)
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ConflictData {
    pub date: Option<NaiveDateWrapper>,
    pub datetime: Option<DateTimeWrapper>,
}

#[derive(RwebResponse)]
#[response(description = "List Conflicts", content = "html")]
struct ListConflictsResponse(HtmlBase<String, Error>);

#[get("/api/list_conflicts")]
pub async fn list_conflicts(
    query: Query<ConflictData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ListConflictsResponse> {
    let query = query.into_inner();
    let body = list_conflicts_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn list_conflicts_body(query: ConflictData, state: AppState) -> HttpResult<String> {
    let diary_date = query.date.map(Into::into);
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

#[derive(RwebResponse)]
#[response(description = "Show Conflict", content = "html")]
struct ShowConflictResponse(HtmlBase<String, Error>);

#[get("/api/show_conflict")]
pub async fn show_conflict(
    query: Query<ConflictData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ShowConflictResponse> {
    let query = query.into_inner();
    let body = show_conflict_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn show_conflict_body(query: ConflictData, state: AppState) -> HttpResult<String> {
    let datetime = query.datetime.map_or_else(Utc::now, Into::into);
    let diary_date = query.date.map_or_else(
        || datetime.with_timezone(&Local).naive_local().date(),
        Into::into,
    );
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

#[derive(RwebResponse)]
#[response(description = "Remove Conflict", content = "html")]
struct RemoveConflictResponse(HtmlBase<String, Error>);

#[get("/api/remove_conflict")]
pub async fn remove_conflict(
    query: Query<ConflictData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<RemoveConflictResponse> {
    let query = query.into_inner();
    let body = remove_conflict_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn remove_conflict_body(query: ConflictData, state: AppState) -> HttpResult<String> {
    let body = if let Some(datetime) = query.datetime {
        let text = DiaryAppRequests::RemoveConflict(datetime.into())
            .handle(&state.db)
            .await?;
        text.join("\n")
    } else if let Some(date) = query.date {
        let text = DiaryAppRequests::CleanConflicts(date.into())
            .handle(&state.db)
            .await?;
        text.join("\n")
    } else {
        "".to_string()
    };
    Ok(body)
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ConflictUpdateData {
    pub id: i32,
    pub diff_type: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Update Conflict", content = "html")]
struct UpdateConflictResponse(HtmlBase<String, Error>);

#[get("/api/update_conflict")]
pub async fn update_conflict(
    query: Query<ConflictUpdateData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<UpdateConflictResponse> {
    let query = query.into_inner();
    update_conflict_body(query, state).await?;
    Ok(HtmlBase::new("finished".to_string()).into())
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

#[derive(Serialize, Deserialize, Schema)]
pub struct CommitConflictData {
    pub datetime: DateTimeWrapper,
}

#[derive(RwebResponse)]
#[response(description = "Commit Conflict")]
struct ConflictResponse(JsonBase<ReplaceOutput, Error>);

#[get("/api/commit_conflict")]
pub async fn commit_conflict(
    query: Query<CommitConflictData>,
    #[cookie = "jwt"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ConflictResponse> {
    let query = query.into_inner();
    let body = commit_conflict_body(query, state).await?;
    let entry = body.join("\n");
    Ok(JsonBase::new(ReplaceOutput { entry }).into())
}

async fn commit_conflict_body(
    query: CommitConflictData,
    state: AppState,
) -> HttpResult<Vec<StackString>> {
    DiaryAppRequests::CommitConflict(query.datetime.into())
        .handle(&state.db)
        .await
        .map_err(Into::into)
}

#[derive(RwebResponse)]
#[response(description = "Logged in User")]
struct UserResponse(JsonBase<LoggedUser, Error>);

#[get("/api/user")]
pub async fn user(#[cookie = "jwt"] user: LoggedUser) -> WarpResult<UserResponse> {
    Ok(JsonBase::new(user).into())
}
