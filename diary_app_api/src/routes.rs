use itertools::Itertools;
use log::debug;
use maplit::hashmap;
use rweb::{get, post, Json, Query, Rejection, Schema};
use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, DateType,
    RwebResponse, UuidWrapper,
};
use serde::{Deserialize, Serialize};
use stack_string::{format_sstr, StackString};
use std::collections::HashSet;
use time::{macros::format_description, Date, OffsetDateTime};
use time_tz::OffsetDateTimeExt;

use diary_app_lib::{date_time_wrapper::DateTimeWrapper, models::DiaryCache};

use super::{
    app::AppState,
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    requests::{DiaryAppOutput, DiaryAppRequests, ListOptions, SearchOptions},
    CommitConflictData, ConflictData, DiaryCacheWrapper,
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
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SearchApiResponse> {
    let query = query.into_inner();
    let body = search_api_body(query, state).await?;
    let text = body.join("\n");
    Ok(JsonBase::new(SearchApiOutput { text }).into())
}

async fn search_api_body(query: SearchOptions, state: AppState) -> HttpResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Search(query).handle(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad Output".into()))
    }
}

#[derive(RwebResponse)]
#[response(description = "Search Output", content = "html")]
struct SearchResponse(HtmlBase<StackString, Error>);

#[get("/api/search")]
pub async fn search(
    query: Query<SearchOptions>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SearchResponse> {
    let query = query.into_inner();
    let body = search_body(query, state).await?;
    let body = format_sstr!(
        r#"<textarea autofocus readonly="readonly"
            name="message" id="diary_editor_form"
            rows=50 cols=100>{}</textarea>"#,
        body.join("\n")
    );
    Ok(HtmlBase::new(body).into())
}

async fn search_body(query: SearchOptions, state: AppState) -> HttpResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Search(query).handle(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad Output".into()))
    }
}

#[derive(Serialize, Deserialize, Schema)]
pub struct InsertData {
    #[schema(description = "Text to Insert")]
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
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<InsertDataResponse> {
    let data = data.into_inner();
    let body = insert_body(data, state).await?;
    let datetime = body.join("\n");
    Ok(JsonBase::new(InsertDataOutput { datetime }).into())
}

async fn insert_body(data: InsertData, state: AppState) -> HttpResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Insert(data.text)
        .handle(&state.db)
        .await?
    {
        Ok(body)
    } else {
        Err(Error::BadRequest("Wrong output".into()))
    }
}

#[derive(RwebResponse)]
#[response(description = "Sync Output", content = "html")]
struct SyncResponse(HtmlBase<StackString, Error>);

#[get("/api/sync")]
pub async fn sync(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SyncResponse> {
    let body = sync_body(state).await?;
    let body = format_sstr!(
        r#"<textarea autofocus readonly="readonly" name="message" id="diary_editor_form" rows=50 cols=100>{}</textarea>"#,
        body.join("\n")
    );
    Ok(HtmlBase::new(body).into())
}

async fn sync_body(state: AppState) -> HttpResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Sync.handle(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad output".into()))
    }
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
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SyncApiResponse> {
    let body = sync_body(state).await?;
    let response = body.join("\n");
    Ok(JsonBase::new(SyncApiOutput { response }).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ReplaceData {
    #[schema(description = "Replacement Date")]
    pub date: DateType,
    #[schema(description = "Replacement Text")]
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
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ReplaceResponse> {
    let data = data.into_inner();
    let body = replace_body(data, state).await?;
    let entry = body.join("\n");
    Ok(JsonBase::new(ReplaceOutput { entry }).into())
}

async fn replace_body(data: ReplaceData, state: AppState) -> HttpResult<Vec<StackString>> {
    let req = DiaryAppRequests::Replace {
        date: data.date.into(),
        text: data.text,
    };
    if let DiaryAppOutput::Lines(body) = req.handle(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad output".into()))
    }
}

fn _list_string(
    conflicts: &HashSet<DateType>,
    body: impl IntoIterator<Item = DateType>,
    query: ListOptions,
) -> StackString {
    let text = body
        .into_iter()
        .map(|t| {
            let d: Date = t.into();
            format_sstr!(
                r#"
                    <input type="button"
                        type="submit"
                        name="{d}"
                        value="{d}"
                        onclick="switchToDate( '{d}' )">{c}
                    <br>"#,
                c = if conflicts.contains(&t) {
                    format_sstr!(
                        r#"
                            <input type="button"
                                type="submit"
                                name="conflict_{d}"
                                value="Conflict {d}"
                                onclick="listConflicts( '{d}' )"
                            >"#,
                    )
                } else {
                    "".into()
                }
            )
        })
        .join("\n");
    let buttons = if query.start.is_some() {
        vec![
            format_sstr!(
                r#"<button type="submit" onclick="gotoEntries({})">Previous</button>"#,
                -10
            ),
            format_sstr!(
                r#"<button type="submit" onclick="gotoEntries({})">Next</button>"#,
                10
            ),
        ]
        .join("\n")
    } else {
        vec![format_sstr!(
            r#"<button type="submit" onclick="gotoEntries({})">Next</button>"#,
            10
        )]
        .join("\n")
    };
    format_sstr!("{text}\n<br>\n{buttons}")
}

#[derive(RwebResponse)]
#[response(description = "List Output", content = "html")]
struct ListResponse(HtmlBase<StackString, Error>);

#[get("/api/list")]
pub async fn list(
    query: Query<ListOptions>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ListResponse> {
    let query = query.into_inner();
    let body = list_body(query, &state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn list_body(query: ListOptions, state: &AppState) -> HttpResult<StackString> {
    let body = list_api_body(query, state).await?;
    let conflicts = if let DiaryAppOutput::Dates(dates) = DiaryAppRequests::ListConflicts(None)
        .handle(&state.db)
        .await?
    {
        dates.into_iter().map(Into::into).collect()
    } else {
        HashSet::new()
    };
    let body = _list_string(&conflicts, body, query);
    Ok(body)
}

async fn list_api_body(query: ListOptions, state: &AppState) -> HttpResult<Vec<DateType>> {
    if let DiaryAppOutput::Dates(dates) = DiaryAppRequests::List(query).handle(&state.db).await? {
        Ok(dates.into_iter().map(Into::into).collect())
    } else {
        Err(Error::BadRequest("Bad results".into()))
    }
}

#[derive(Schema, Serialize)]
struct ListOutput {
    list: Vec<DateType>,
}

#[derive(RwebResponse)]
#[response(description = "ListApi Response")]
struct ListApiResponse(JsonBase<ListOutput, Error>);

#[get("/api/list_api")]
pub async fn list_api(
    query: Query<ListOptions>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ListApiResponse> {
    let query = query.into_inner();
    let list = list_api_body(query, &state).await?;
    Ok(JsonBase::new(ListOutput { list }).into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct EditData {
    pub date: DateType,
}

#[derive(RwebResponse)]
#[response(description = "Edit Output", content = "html")]
struct EditResponse(HtmlBase<StackString, Error>);

#[get("/api/edit")]
pub async fn edit(
    query: Query<EditData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<EditResponse> {
    let query = query.into_inner();
    let body = edit_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn edit_body(query: EditData, state: AppState) -> HttpResult<StackString> {
    let diary_date = query.date.into();
    let text = if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::Display(diary_date)
        .handle(&state.db)
        .await?
    {
        lines
    } else {
        Vec::new()
    };
    let body = format_sstr!(
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
struct DisplayResponse(HtmlBase<StackString, Error>);

#[get("/api/display")]
pub async fn display(
    query: Query<EditData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<DisplayResponse> {
    let query = query.into_inner();
    let body = display_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn display_body(query: EditData, state: AppState) -> HttpResult<StackString> {
    let diary_date = query.date.into();
    let text = if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::Display(diary_date)
        .handle(&state.db)
        .await?
    {
        lines
    } else {
        Vec::new()
    };
    let body = format_sstr!(
        r#"<textarea autofocus readonly="readonly" name="message" id="diary_editor_form" rows=50 cols=100>{text}</textarea><br>{editor}"#,
        text = text.join("\n"),
        editor = format_sstr!(
            r#"<input type="button" name="edit" value="Edit" onclick="switchToEditor('{}')">"#,
            diary_date
        ),
    );
    Ok(body)
}

#[derive(RwebResponse)]
#[response(description = "Frontpage", content = "html")]
struct FrontpageResponse(HtmlBase<StackString, Error>);

#[get("/api/index.html")]
pub async fn diary_frontpage(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<FrontpageResponse> {
    let body = diary_frontpage_body(state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn diary_frontpage_body(state: AppState) -> HttpResult<StackString> {
    let query = ListOptions {
        limit: Some(10),
        ..ListOptions::default()
    };
    let body = if let DiaryAppOutput::Dates(dates) =
        DiaryAppRequests::List(query).handle(&state.db).await?
    {
        dates.into_iter().map(Into::into).collect()
    } else {
        Vec::new()
    };
    debug!("Got list");
    let conflicts = if let DiaryAppOutput::Dates(dates) = DiaryAppRequests::ListConflicts(None)
        .handle(&state.db)
        .await?
    {
        dates.into_iter().map(Into::into).collect()
    } else {
        HashSet::new()
    };
    debug!("Got conflicts");
    let body = _list_string(&conflicts, body, query);
    let params = hashmap! {
        "LIST_TEXT" => body.as_str(),
        "DISPLAY_TEXT" => "",
    };
    let body = state.hb.render("id", &params)?.into();
    Ok(body)
}

#[derive(RwebResponse)]
#[response(description = "List Conflicts", content = "html")]
struct ListConflictsResponse(HtmlBase<StackString, Error>);

#[get("/api/list_conflicts")]
pub async fn list_conflicts(
    query: Query<ConflictData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ListConflictsResponse> {
    let query = query.into_inner();
    let body = list_conflicts_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn list_conflicts_body(query: ConflictData, state: AppState) -> HttpResult<StackString> {
    let body = if let DiaryAppOutput::Timestamps(dates) =
        DiaryAppRequests::ListConflicts(query.date)
            .handle(&state.db)
            .await?
    {
        dates
    } else {
        Vec::new()
    };
    let mut buttons = Vec::new();
    if let Some(date) = query.date {
        if !body.is_empty() {
            let date: Date = date.into();
            buttons.push(format_sstr!(
                r#"<button type="submit" onclick="cleanConflicts('{}')">Clean</button>"#,
                date
            ));
        }
    }
    buttons.push(r#"<button type="submit" onclick="switchToList()">List</button>"#.into());

    let local = DateTimeWrapper::local_tz();
    let text = body
        .into_iter()
        .map(|t| {
            format_sstr!(
                r#"
            <input type="button"
                type="submit"
                name="show_{t}"
                value="Show {t}"
                onclick="showConflict( '{d}', '{t}' )">
            <br>
        "#,
                t = t,
                d = query
                    .date
                    .unwrap_or_else(|| OffsetDateTime::now_utc().to_timezone(local).date().into())
                    .to_string(),
            )
        })
        .join("\n");

    let body = format_sstr!("{}\n<br>\n{}", text, buttons.join("<br>"));
    Ok(body)
}

#[derive(RwebResponse)]
#[response(description = "Show Conflict", content = "html")]
struct ShowConflictResponse(HtmlBase<StackString, Error>);

#[get("/api/show_conflict")]
pub async fn show_conflict(
    query: Query<ConflictData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ShowConflictResponse> {
    let query = query.into_inner();
    let body = show_conflict_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn show_conflict_body(query: ConflictData, state: AppState) -> HttpResult<StackString> {
    let local = DateTimeWrapper::local_tz();
    let datetime = query
        .datetime
        .unwrap_or_else(|| OffsetDateTime::now_utc().into());
    let diary_date: Date = query
        .date
        .unwrap_or_else(|| OffsetDateTime::now_utc().to_timezone(local).date().into())
        .into();
    let text = if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::ShowConflict(datetime)
        .handle(&state.db)
        .await?
    {
        lines
    } else {
        Vec::new()
    };
    let body = format_sstr!(
        r#"{t}<br>
            <input type="button" name="display" value="Display" onclick="switchToDisplay('{d}')">
            <input type="button" name="commit" value="Commit" onclick="commitConflict('{d}', '{dt}')">
            <input type="button" name="remove" value="Remove" onclick="removeConflict('{d}', '{dt}')">
            <input type="button" name="edit" value="Edit" onclick="switchToEditor('{d}')">
            "#,
        t = text.join("\n"),
        d = diary_date,
        dt = datetime
            .format(format_description!(
                "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]Z"
            ))
            .unwrap_or_else(|_| "".into()),
    );
    Ok(body)
}

#[derive(RwebResponse)]
#[response(description = "Remove Conflict", content = "html")]
struct RemoveConflictResponse(HtmlBase<StackString, Error>);

#[get("/api/remove_conflict")]
pub async fn remove_conflict(
    query: Query<ConflictData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<RemoveConflictResponse> {
    let query = query.into_inner();
    let body = remove_conflict_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn remove_conflict_body(query: ConflictData, state: AppState) -> HttpResult<StackString> {
    let body = if let Some(datetime) = query.datetime {
        if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::RemoveConflict(datetime)
            .handle(&state.db)
            .await?
        {
            lines.join("\n")
        } else {
            String::new()
        }
    } else if let Some(date) = query.date {
        if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::CleanConflicts(date.into())
            .handle(&state.db)
            .await?
        {
            lines.join("\n")
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    Ok(body.into())
}

#[derive(Serialize, Deserialize, Schema)]
pub struct ConflictUpdateData {
    #[schema(description = "Conflict ID")]
    pub id: UuidWrapper,
    #[schema(description = "Difference Type")]
    pub diff_type: StackString,
}

#[derive(RwebResponse)]
#[response(description = "Update Conflict", content = "html")]
struct UpdateConflictResponse(HtmlBase<&'static str, Error>);

#[get("/api/update_conflict")]
pub async fn update_conflict(
    query: Query<ConflictUpdateData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<UpdateConflictResponse> {
    let query = query.into_inner();
    update_conflict_body(query, state).await?;
    Ok(HtmlBase::new("finished").into())
}

async fn update_conflict_body(query: ConflictUpdateData, state: AppState) -> HttpResult<()> {
    DiaryAppRequests::UpdateConflict {
        id: query.id.into(),
        diff_text: query.diff_type,
    }
    .handle(&state.db)
    .await?;
    Ok(())
}

#[derive(RwebResponse)]
#[response(description = "Commit Conflict")]
struct ConflictResponse(JsonBase<ReplaceOutput, Error>);

#[get("/api/commit_conflict")]
pub async fn commit_conflict(
    query: Query<CommitConflictData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
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
    if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::CommitConflict(query.datetime)
        .handle(&state.db)
        .await?
    {
        Ok(lines)
    } else {
        Ok(Vec::new())
    }
}

#[derive(RwebResponse)]
#[response(description = "Logged in User")]
struct UserResponse(JsonBase<LoggedUser, Error>);

#[get("/api/user")]
pub async fn user(#[filter = "LoggedUser::filter"] user: LoggedUser) -> WarpResult<UserResponse> {
    Ok(JsonBase::new(user).into())
}

#[derive(RwebResponse)]
#[response(description = "Get Diary Cache")]
struct DiaryCacheResponse(JsonBase<Vec<DiaryCacheWrapper>, Error>);

#[get("/api/diary_cache")]
pub async fn diary_cache(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<DiaryCacheResponse> {
    let cache_entries: Vec<_> = DiaryCache::get_cache_entries(&state.db.pool)
        .await
        .map_err(Into::<Error>::into)?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok(JsonBase::new(cache_entries).into())
}

#[derive(RwebResponse)]
#[response(description = "Cache Update Response")]
struct DiaryCacheUpdateResponse(HtmlBase<&'static str, Error>);

#[post("/api/diary_cache")]
pub async fn diary_cache_update(
    payload: Json<Vec<DiaryCacheWrapper>>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<DiaryCacheUpdateResponse> {
    for entry in payload.into_inner() {
        let entry: DiaryCache = entry.into();
        entry
            .insert_entry(&state.db.pool)
            .await
            .map_err(Into::<Error>::into)?;
    }
    Ok(HtmlBase::new("finished").into())
}
