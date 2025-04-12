use axum::extract::{Json, Query, State};
use serde::{Deserialize, Serialize};
use stack_string::StackString;
use std::{collections::HashSet, sync::Arc};
use time::{Date, OffsetDateTime};
use time_tz::OffsetDateTimeExt;
use utoipa::{IntoParams, OpenApi, PartialSchema, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};
use utoipa_helper::{
    UtoipaResponse, html_response::HtmlResponse as HtmlBase,
    json_response::JsonResponse as JsonBase,
};
use uuid::Uuid;

use diary_app_lib::date_time_wrapper::DateTimeWrapper;

use super::{
    CommitConflictData, ConflictData,
    app::AppState,
    elements::{
        edit_body, index_body, list_body, list_conflicts_body, search_body, show_conflict_body,
    },
    errors::ServiceError as Error,
    logged_user::LoggedUser,
    requests::{DiaryAppOutput, DiaryAppRequests, ListOptions, SearchOptions},
};

type AxumResult<T> = Result<T, Error>;

#[derive(UtoipaResponse)]
#[response(description = "Search Output", content = "text/html")]
#[rustfmt::skip]
struct SearchResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/api/search",
    params(SearchOptions),
    responses(SearchResponse, Error)
)]
// Search Output Page
async fn search(
    query: Query<SearchOptions>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<SearchResponse> {
    let Query(query) = query;
    let results = search_results(query, &state).await?;
    let body = search_body(results)?.into();
    Ok(HtmlBase::new(body).into())
}

async fn search_results(query: SearchOptions, state: &AppState) -> AxumResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Search(query).process(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad Output".into()))
    }
}

#[derive(Serialize, Deserialize, ToSchema)]
// InsertData
struct InsertData {
    // Text to Insert
    #[schema(inline)]
    text: StackString,
}

#[derive(ToSchema, Serialize)]
struct InsertDataOutput {
    datetime: String,
}

#[derive(UtoipaResponse)]
#[response(description = "Insert Data Result", status = "CREATED")]
#[rustfmt::skip]
struct InsertDataResponse(JsonBase::<InsertDataOutput>);

#[utoipa::path(post, path = "/api/insert", request_body = InsertData, responses(InsertDataResponse, Error))]
// Insert Text into Cache
async fn insert(
    state: State<Arc<AppState>>,
    _: LoggedUser,
    data: Json<InsertData>,
) -> AxumResult<InsertDataResponse> {
    let Json(data) = data;
    let body = insert_body(data, &state).await?;
    let datetime = body.join("\n");
    Ok(JsonBase::new(InsertDataOutput { datetime }).into())
}

async fn insert_body(data: InsertData, state: &AppState) -> AxumResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Insert(data.text)
        .process(&state.db)
        .await?
    {
        Ok(body)
    } else {
        Err(Error::BadRequest("Wrong output".into()))
    }
}

#[derive(UtoipaResponse)]
#[response(description = "Sync Output", content = "text/html")]
#[rustfmt::skip]
struct SyncResponse(HtmlBase::<StackString>);

#[utoipa::path(post, path = "/api/sync", responses(SyncResponse, Error))]
// Sync Diary
async fn sync(_: LoggedUser, state: State<Arc<AppState>>) -> AxumResult<SyncResponse> {
    let results = sync_body(&state).await?;
    let body = search_body(results)?.into();
    Ok(HtmlBase::new(body).into())
}

async fn sync_body(state: &AppState) -> AxumResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(body) = DiaryAppRequests::Sync.process(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad output".into()))
    }
}

#[derive(Serialize, Deserialize, ToSchema)]
// ReplaceData
struct ReplaceData {
    // Replacement Date
    date: Date,
    // Replacement Text
    text: StackString,
}

#[derive(ToSchema, Serialize)]
struct ReplaceOutput {
    entry: String,
}

#[derive(UtoipaResponse)]
#[response(description = "Replace Response", status = "CREATED")]
#[rustfmt::skip]
struct ReplaceResponse(JsonBase::<ReplaceOutput>);

#[utoipa::path(post, path = "/api/replace", request_body = ReplaceData, responses(ReplaceResponse, Error))]
// Insert Text at Specific Date, replace existing text
async fn replace(
    state: State<Arc<AppState>>,
    _: LoggedUser,
    data: Json<ReplaceData>,
) -> AxumResult<ReplaceResponse> {
    let Json(data) = data;
    let body = replace_body(data, &state).await?;
    let entry = body.join("\n");
    Ok(JsonBase::new(ReplaceOutput { entry }).into())
}

async fn replace_body(data: ReplaceData, state: &AppState) -> AxumResult<Vec<StackString>> {
    let req = DiaryAppRequests::Replace {
        date: data.date,
        text: data.text,
    };
    if let DiaryAppOutput::Lines(body) = req.process(&state.db).await? {
        Ok(body)
    } else {
        Err(Error::BadRequest("Bad output".into()))
    }
}

#[derive(UtoipaResponse)]
#[response(description = "List Output", content = "text/html")]
#[rustfmt::skip]
struct ListResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/api/list",
    params(ListOptions),
    responses(ListResponse, Error)
)]
// List of Date Buttons
async fn list(
    query: Query<ListOptions>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<ListResponse> {
    let Query(query) = query;
    let body = get_body(query, &state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn get_body(query: ListOptions, state: &AppState) -> AxumResult<StackString> {
    let dates = list_api_body(query, state).await?;
    let conflicts = if let DiaryAppOutput::Dates(d) = DiaryAppRequests::ListConflicts(None)
        .process(&state.db)
        .await?
    {
        d.into_iter().collect()
    } else {
        HashSet::new()
    };
    let body = list_body(conflicts, dates, query.start)?.into();
    Ok(body)
}

async fn list_api_body(query: ListOptions, state: &AppState) -> AxumResult<Vec<Date>> {
    if let DiaryAppOutput::Dates(dates) = DiaryAppRequests::List(query).process(&state.db).await? {
        Ok(dates)
    } else {
        Err(Error::BadRequest("Bad results".into()))
    }
}

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct EditData {
    date: Date,
}

#[derive(UtoipaResponse)]
#[response(description = "Edit Output", content = "text/html")]
#[rustfmt::skip]
struct EditResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/api/edit",
    params(EditData),
    responses(EditResponse, Error)
)]
// Diary Edit Form
async fn edit(
    query: Query<EditData>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<EditResponse> {
    let Query(query) = query;
    let body = get_edit_body(query, &state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn get_edit_body(query: EditData, state: &AppState) -> AxumResult<StackString> {
    let diary_date = query.date;
    let text = if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::Display(diary_date)
        .process(&state.db)
        .await?
    {
        lines
    } else {
        Vec::new()
    };
    let body = edit_body(diary_date, text, false)?.into();
    Ok(body)
}

#[derive(UtoipaResponse)]
#[response(description = "Display Output", content = "text/html")]
#[rustfmt::skip]
struct DisplayResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/api/display",
    params(EditData),
    responses(DisplayResponse, Error)
)]
// Display Diary Entry
async fn display(
    query: Query<EditData>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<DisplayResponse> {
    let Query(query) = query;
    let body = display_body(query, &state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn display_body(query: EditData, state: &AppState) -> AxumResult<StackString> {
    let diary_date = query.date;
    let text = if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::Display(diary_date)
        .process(&state.db)
        .await?
    {
        lines
    } else {
        Vec::new()
    };
    let body = edit_body(diary_date, text, true)?.into();
    Ok(body)
}

#[derive(UtoipaResponse)]
#[response(description = "Frontpage", content = "text/html")]
#[rustfmt::skip]
struct FrontpageResponse(HtmlBase::<StackString>);

#[utoipa::path(get, path = "/api/index.html", responses(FrontpageResponse, Error))]
// Diary Main Page
async fn diary_frontpage(_: LoggedUser) -> AxumResult<FrontpageResponse> {
    let body = index_body()?.into();
    Ok(HtmlBase::new(body).into())
}

#[derive(UtoipaResponse)]
#[response(description = "List Conflicts", content = "text/html")]
#[rustfmt::skip]
struct ListConflictsResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/api/list_conflicts",
    params(ConflictData),
    responses(ListConflictsResponse, Error)
)]
// List Conflicts
async fn list_conflicts(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<ListConflictsResponse> {
    let Query(query) = query;
    let body = get_conflicts_body(query, &state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn get_conflicts_body(query: ConflictData, state: &AppState) -> AxumResult<StackString> {
    let conflicts = if let DiaryAppOutput::Timestamps(dates) =
        DiaryAppRequests::ListConflicts(query.date)
            .process(&state.db)
            .await?
    {
        dates
    } else {
        Vec::new()
    };
    let body = list_conflicts_body(query.date, conflicts)?.into();
    Ok(body)
}

#[derive(UtoipaResponse)]
#[response(description = "Show Conflict", content = "text/html")]
#[rustfmt::skip]
struct ShowConflictResponse(HtmlBase::<StackString>);

#[utoipa::path(
    get,
    path = "/api/show_conflict",
    params(ConflictData),
    responses(ShowConflictResponse, Error)
)]
// Show Conflict
async fn show_conflict(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<ShowConflictResponse> {
    let Query(query) = query;
    let body = get_show_conflict(query, &state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn get_show_conflict(query: ConflictData, state: &AppState) -> AxumResult<StackString> {
    let local = DateTimeWrapper::local_tz();
    let datetime = query
        .datetime
        .unwrap_or_else(|| OffsetDateTime::now_utc().into());
    let diary_date: Date = query
        .date
        .unwrap_or_else(|| OffsetDateTime::now_utc().to_timezone(local).date());
    let conflicts = if let DiaryAppOutput::Conflicts(conflicts) =
        DiaryAppRequests::ShowConflict(datetime)
            .process(&state.db)
            .await?
    {
        conflicts
    } else {
        Vec::new()
    };
    let body = show_conflict_body(diary_date, conflicts, datetime)?.into();
    Ok(body)
}

#[derive(UtoipaResponse)]
#[response(description = "Remove Conflict", content = "text/html")]
#[rustfmt::skip]
struct RemoveConflictResponse(HtmlBase::<StackString>);

#[utoipa::path(
    delete,
    path = "/api/remove_conflict",
    params(ConflictData),
    responses(RemoveConflictResponse, Error)
)]
// Delete Conflict
async fn remove_conflict(
    query: Query<ConflictData>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<RemoveConflictResponse> {
    let Query(query) = query;
    let body = remove_conflict_body(query, &state).await?;
    Ok(HtmlBase::new(body).into())
}

async fn remove_conflict_body(query: ConflictData, state: &AppState) -> AxumResult<StackString> {
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
        if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::CleanConflicts(date)
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

#[derive(Serialize, Deserialize, ToSchema, IntoParams)]
struct ConflictUpdateData {
    // Conflict ID
    id: Uuid,
    // Difference Type
    #[schema(inline)]
    #[param(inline)]
    diff_type: StackString,
}

#[derive(UtoipaResponse)]
#[response(description = "Update Conflict", content = "text/html")]
#[rustfmt::skip]
struct UpdateConflictResponse(HtmlBase::<&'static str>);

#[utoipa::path(
    patch,
    path = "/api/update_conflict",
    params(ConflictUpdateData),
    responses(UpdateConflictResponse, Error)
)]
// Update Conflict
async fn update_conflict(
    query: Query<ConflictUpdateData>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<UpdateConflictResponse> {
    let Query(query) = query;
    update_conflict_body(query, &state).await?;
    Ok(HtmlBase::new("finished").into())
}

async fn update_conflict_body(query: ConflictUpdateData, state: &AppState) -> AxumResult<()> {
    DiaryAppRequests::UpdateConflict {
        id: query.id,
        diff_text: query.diff_type,
    }
    .process(&state.db)
    .await?;
    Ok(())
}

#[derive(UtoipaResponse)]
#[response(description = "Commit Conflict")]
#[rustfmt::skip]
struct ConflictResponse(JsonBase::<ReplaceOutput>);

#[utoipa::path(
    post,
    path = "/api/commit_conflict",
    params(CommitConflictData),
    responses(ConflictResponse, Error)
)]
// Commit Conflict
async fn commit_conflict(
    query: Query<CommitConflictData>,
    _: LoggedUser,
    state: State<Arc<AppState>>,
) -> AxumResult<ConflictResponse> {
    let Query(query) = query;
    let body = commit_conflict_body(query, &state).await?;
    let entry = body.join("\n");
    Ok(JsonBase::new(ReplaceOutput { entry }).into())
}

async fn commit_conflict_body(
    query: CommitConflictData,
    state: &AppState,
) -> AxumResult<Vec<StackString>> {
    if let DiaryAppOutput::Lines(lines) = DiaryAppRequests::CommitConflict(query.datetime)
        .process(&state.db)
        .await?
    {
        Ok(lines)
    } else {
        Ok(Vec::new())
    }
}

#[derive(UtoipaResponse)]
#[response(description = "Logged in User")]
#[rustfmt::skip]
struct UserResponse(JsonBase::<LoggedUser>);

#[utoipa::path(get, path = "/api/user", responses(UserResponse, Error))]
// Get User Object
async fn user(user: LoggedUser) -> AxumResult<UserResponse> {
    Ok(JsonBase::new(user).into())
}

pub fn get_api_path(app: &AppState) -> OpenApiRouter {
    let app = Arc::new(app.clone());

    OpenApiRouter::new()
        .routes(routes!(search))
        .routes(routes!(insert))
        .routes(routes!(sync))
        .routes(routes!(replace))
        .routes(routes!(list))
        .routes(routes!(edit))
        .routes(routes!(display))
        .routes(routes!(diary_frontpage))
        .routes(routes!(list_conflicts))
        .routes(routes!(show_conflict))
        .routes(routes!(remove_conflict))
        .routes(routes!(update_conflict))
        .routes(routes!(commit_conflict))
        .routes(routes!(user))
        .with_state(app)
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Frontend for Diary",
        description = "Web Frontend for Diary Service",
    ),
    components(schemas(LoggedUser))
)]
pub struct ApiDoc;
