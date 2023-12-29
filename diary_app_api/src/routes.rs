use rweb::{delete, get, patch, post, Json, Query, Rejection, Schema};
use rweb_helper::{
    html_response::HtmlResponse as HtmlBase, json_response::JsonResponse as JsonBase, DateType,
    RwebResponse, UuidWrapper,
};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::collections::HashSet;
use time::{Date, OffsetDateTime};
use time_tz::OffsetDateTimeExt;

use diary_app_lib::date_time_wrapper::DateTimeWrapper;

use super::{
    app::AppState,
    elements::{
        edit_body, index_body, list_body, list_conflicts_body, search_body, show_conflict_body,
    },
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    requests::{DiaryAppOutput, DiaryAppRequests, ListOptions, SearchOptions},
    CommitConflictData, ConflictData,
};

pub type WarpResult<T> = Result<T, Rejection>;
pub type HttpResult<T> = Result<T, Error>;

#[derive(RwebResponse)]
#[response(description = "Search Output", content = "html")]
struct SearchResponse(HtmlBase<StackString, Error>);

#[get("/api/search")]
#[openapi(description = "Search Output Page")]
pub async fn search(
    query: Query<SearchOptions>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SearchResponse> {
    let query = query.into_inner();
    let results = search_results(query, state).await?;
    let body = search_body(results).into();
    Ok(HtmlBase::new(body).into())
}

async fn search_results(query: SearchOptions, state: AppState) -> HttpResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Search(query).process(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad Output".into()))
    }
}

#[derive(Serialize, Deserialize, Schema)]
#[schema(component = "InsertData")]
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
#[openapi(description = "Insert Text into Cache")]
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
        .process(&state.db)
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

#[post("/api/sync")]
#[openapi(description = "Sync Diary")]
pub async fn sync(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<SyncResponse> {
    let results = sync_body(state).await?;
    let body = search_body(results).into();
    Ok(HtmlBase::new(body).into())
}

async fn sync_body(state: AppState) -> HttpResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Sync.process(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad output".into()))
    }
}

#[derive(Serialize, Deserialize, Schema)]
#[schema(component = "ReplaceData")]
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
#[openapi(description = "Insert Text at Specific Date, replace existing text")]
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
    if let DiaryAppOutput::Lines(body) = req.process(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad output".into()))
    }
}

#[derive(RwebResponse)]
#[response(description = "List Output", content = "html")]
struct ListResponse(HtmlBase<StackString, Error>);

#[get("/api/list")]
#[openapi(description = "List of Date Buttons")]
pub async fn list(
    query: Query<ListOptions>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ListResponse> {
    let query = query.into_inner();
    let body = get_body(query, &state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn get_body(query: ListOptions, state: &AppState) -> HttpResult<StackString> {
    let dates = list_api_body(query, state).await?;
    let conflicts = if let DiaryAppOutput::Dates(d) = DiaryAppRequests::ListConflicts(None)
        .process(&state.db)
        .await?
    {
        d.into_iter().map(Into::into).collect()
    } else {
        HashSet::new()
    };
    let body = list_body(conflicts, dates, query.start).into();
    Ok(body)
}

async fn list_api_body(query: ListOptions, state: &AppState) -> HttpResult<Vec<DateType>> {
    if let DiaryAppOutput::Dates(dates) = DiaryAppRequests::List(query).process(&state.db).await? {
        Ok(dates.into_iter().map(Into::into).collect())
    } else {
        Err(Error::BadRequest("Bad results".into()))
    }
}

#[derive(Serialize, Deserialize, Schema)]
pub struct EditData {
    pub date: DateType,
}

#[derive(RwebResponse)]
#[response(description = "Edit Output", content = "html")]
struct EditResponse(HtmlBase<StackString, Error>);

#[get("/api/edit")]
#[openapi(description = "Diary Edit Form")]
pub async fn edit(
    query: Query<EditData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<EditResponse> {
    let query = query.into_inner();
    let body = get_edit_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn get_edit_body(query: EditData, state: AppState) -> HttpResult<StackString> {
    let diary_date = query.date.into();
    let text = if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::Display(diary_date)
        .process(&state.db)
        .await?
    {
        lines
    } else {
        Vec::new()
    };
    let body = edit_body(diary_date, text, false).into();
    Ok(body)
}

#[derive(RwebResponse)]
#[response(description = "Display Output", content = "html")]
struct DisplayResponse(HtmlBase<StackString, Error>);

#[get("/api/display")]
#[openapi(description = "Display Diary Entry")]
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
        .process(&state.db)
        .await?
    {
        lines
    } else {
        Vec::new()
    };
    let body = edit_body(diary_date, text, true).into();
    Ok(body)
}

#[derive(RwebResponse)]
#[response(description = "Frontpage", content = "html")]
struct FrontpageResponse(HtmlBase<StackString, Error>);

#[get("/api/index.html")]
#[openapi(description = "Diary Main Page")]
pub async fn diary_frontpage(
    #[filter = "LoggedUser::filter"] _: LoggedUser,
) -> WarpResult<FrontpageResponse> {
    let body = index_body().into();
    Ok(HtmlBase::new(body).into())
}

#[derive(RwebResponse)]
#[response(description = "List Conflicts", content = "html")]
struct ListConflictsResponse(HtmlBase<StackString, Error>);

#[get("/api/list_conflicts")]
#[openapi(description = "List Conflicts")]
pub async fn list_conflicts(
    query: Query<ConflictData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ListConflictsResponse> {
    let query = query.into_inner();
    let body = get_conflicts_body(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn get_conflicts_body(query: ConflictData, state: AppState) -> HttpResult<StackString> {
    let conflicts = if let DiaryAppOutput::Timestamps(dates) =
        DiaryAppRequests::ListConflicts(query.date)
            .process(&state.db)
            .await?
    {
        dates
    } else {
        Vec::new()
    };
    let body = list_conflicts_body(query.date, conflicts).into();
    Ok(body)
}

#[derive(RwebResponse)]
#[response(description = "Show Conflict", content = "html")]
struct ShowConflictResponse(HtmlBase<StackString, Error>);

#[get("/api/show_conflict")]
#[openapi(description = "Show Conflict")]
pub async fn show_conflict(
    query: Query<ConflictData>,
    #[filter = "LoggedUser::filter"] _: LoggedUser,
    #[data] state: AppState,
) -> WarpResult<ShowConflictResponse> {
    let query = query.into_inner();
    let body = get_show_conflict(query, state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn get_show_conflict(query: ConflictData, state: AppState) -> HttpResult<StackString> {
    let local = DateTimeWrapper::local_tz();
    let datetime = query
        .datetime
        .unwrap_or_else(|| OffsetDateTime::now_utc().into());
    let diary_date: Date = query
        .date
        .unwrap_or_else(|| OffsetDateTime::now_utc().to_timezone(local).date().into())
        .into();
    let conflicts = if let DiaryAppOutput::Conflicts(conflicts) =
        DiaryAppRequests::ShowConflict(datetime)
            .process(&state.db)
            .await?
    {
        conflicts
    } else {
        Vec::new()
    };
    let body = show_conflict_body(diary_date, conflicts, datetime).into();
    Ok(body)
}

#[derive(RwebResponse)]
#[response(description = "Remove Conflict", content = "html")]
struct RemoveConflictResponse(HtmlBase<StackString, Error>);

#[delete("/api/remove_conflict")]
#[openapi(description = "Delete Conflict")]
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
            .process(&state.db)
            .await?
        {
            lines.join("\n")
        } else {
            String::new()
        }
    } else if let Some(date) = query.date {
        if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::CleanConflicts(date.into())
            .process(&state.db)
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

#[patch("/api/update_conflict")]
#[openapi(description = "Update Conflict")]
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
    .process(&state.db)
    .await?;
    Ok(())
}

#[derive(RwebResponse)]
#[response(description = "Commit Conflict")]
struct ConflictResponse(JsonBase<ReplaceOutput, Error>);

#[post("/api/commit_conflict")]
#[openapi(description = "Commit Conflict")]
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
        .process(&state.db)
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
#[openapi(description = "Get User Object")]
pub async fn user(#[filter = "LoggedUser::filter"] user: LoggedUser) -> WarpResult<UserResponse> {
    Ok(JsonBase::new(user).into())
}
