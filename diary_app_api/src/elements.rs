use dioxus::prelude::{Element, IntoDynNode, Props, VirtualDom, component, dioxus_elements, rsx};
use stack_string::StackString;
use std::collections::{BTreeSet, HashSet};
use time::{Date, OffsetDateTime, macros::format_description};
use time_tz::OffsetDateTimeExt;

#[cfg(debug_assertions)]
use dioxus::prelude::{GlobalSignal, Readable};

use diary_app_lib::{date_time_wrapper::DateTimeWrapper, models::DiaryConflict};

use crate::errors::ServiceError as Error;

/// # Errors
/// Returns error if formatting fails
pub fn index_body() -> Result<String, Error> {
    let mut app = VirtualDom::new(IndexElement);
    app.rebuild_in_place();
    let mut renderer = dioxus_ssr::Renderer::default();
    let mut buffer = String::new();
    renderer
        .render_to(&mut buffer, &app)
        .map_err(Into::<Error>::into)?;
    Ok(buffer)
}

#[component]
fn IndexElement() -> Element {
    rsx! {
        head {
            style {
                dangerous_inner_html: include_str!("../../templates/style.css")
            }
        }
        body {
            form {
                action: "javascript:searchDiary();",
                input {
                    "type": "button",
                    name: "sync_button",
                    value: "Sync",
                    "onclick": "syncDiary();",
                },
                input {
                    "type": "text",
                    name: "search_text",
                    id: "search_text",
                },
                input {
                    "type": "button",
                    name: "search_button",
                    value: "Search",
                    "onclick": "searchDiary();",
                },
                button {
                    name: "diary_status",
                    id: "diary_status",
                    dangerous_inner_html: "&nbsp;",
                },
                br {
                    form {
                        action: "javascript:searchDate();",
                        input {
                            "type": "button",
                            name: "search_date_button",
                            value: "Date",
                            "onclick": "searchDate();",
                        },
                        input {
                            "type": "date",
                            name: "search_date",
                            id: "search_date",
                        }
                    },
                },
            },
            nav {
                id: "navigation",
                "start": "0",
            },
            article {
                id: "main_article",
            },
            script {
                "language": "JavaScript",
                "type": "text/javascript",
                dangerous_inner_html: include_str!("../../templates/scripts.js")
            }
        }
    }
}

/// # Errors
/// Returns error if formatting fails
pub fn list_body(
    conflicts: HashSet<Date>,
    dates: Vec<Date>,
    start: Option<usize>,
) -> Result<String, Error> {
    let mut app = VirtualDom::new_with_props(
        DateListElement,
        DateListElementProps {
            conflicts,
            dates,
            start,
        },
    );
    app.rebuild_in_place();
    let mut renderer = dioxus_ssr::Renderer::default();
    let mut buffer = String::new();
    renderer
        .render_to(&mut buffer, &app)
        .map_err(Into::<Error>::into)?;
    Ok(buffer)
}

#[component]
fn DateListElement(conflicts: HashSet<Date>, dates: Vec<Date>, start: Option<usize>) -> Element {
    let buttons = if start.is_some() {
        rsx! {
            button {
                "type": "submit",
                "onclick": "gotoEntries(-10)",
                "Previous",
            },
            button {
                "type": "submit",
                "onclick": "gotoEntries(10)",
                "Next",
            }
        }
    } else {
        rsx! {
            button {
                "type": "submit",
                "onclick": "gotoEntries(10)",
                "Next",
            }
        }
    };
    rsx! {
        {dates.iter().enumerate().map(|(idx, d)| {
            let c = if conflicts.contains(d) {
                Some(rsx! {
                    input {
                        "type": "submit",
                        name: "conflict_{d}",
                        value: "Conflict {d}",
                        "onclick": "listConflicts( '{d}' )",
                    }
                })
            } else {
                None
            };
            rsx! {
                div {
                    key: "date-key-{idx}",
                    input {
                        "type": "submit",
                        name: "{d}",
                        value: "{d}",
                        "onclick": "switchToDate( '{d}' )",
                        {c}
                    },
                    br {},
                }
            }
        })},
        {buttons},
    }
}

/// # Errors
/// Returns error if formatting fails
pub fn list_conflicts_body(
    date: Option<Date>,
    conflicts: Vec<DateTimeWrapper>,
) -> Result<String, Error> {
    let mut app = VirtualDom::new_with_props(
        ListConflictsElement,
        ListConflictsElementProps { date, conflicts },
    );
    app.rebuild_in_place();
    let mut renderer = dioxus_ssr::Renderer::default();
    let mut buffer = String::new();
    renderer
        .render_to(&mut buffer, &app)
        .map_err(Into::<Error>::into)?;
    Ok(buffer)
}

#[component]
fn ListConflictsElement(date: Option<Date>, conflicts: Vec<DateTimeWrapper>) -> Element {
    let local = DateTimeWrapper::local_tz();
    let clean_conflicts = if let Some(date) = date {
        if conflicts.is_empty() {
            None
        } else {
            Some(rsx! {
                button {
                    "type": "submit",
                    "onclick": "cleanConflicts('{date}')",
                    "Clean"
                }
            })
        }
    } else {
        None
    };
    rsx! {
        {conflicts.iter().enumerate().map(|(idx, t)| {
            let d: Date = date.unwrap_or_else(|| OffsetDateTime::now_utc().to_timezone(local).date());
            rsx! {
                input {
                    key: "show-key-{idx}",
                    "type": "button",
                    name: "show_{t}",
                    value: "Show {t}",
                    "onclick": "showConflict( '{d}', '{t}' )",
                }
            }
        })},
        br {
            {clean_conflicts},
            button {
                "type": "submit",
                "onclick": "switchToList()",
                "List",
            },
        },
    }
}

/// # Errors
/// Returns error if formatting fails
pub fn search_body(results: Vec<StackString>) -> Result<String, Error> {
    let mut app = VirtualDom::new_with_props(SearchElement, SearchElementProps { results });
    app.rebuild_in_place();
    let mut renderer = dioxus_ssr::Renderer::default();
    let mut buffer = String::new();
    renderer
        .render_to(&mut buffer, &app)
        .map_err(Into::<Error>::into)?;
    Ok(buffer)
}

#[component]
fn SearchElement(results: Vec<StackString>) -> Element {
    let body = results.join("\n");
    rsx! {
        textarea {
            "autofocus": "true",
            readonly: "readonly",
            name: "message",
            id: "diary_editor_form",
            "rows": "50",
            "cols": "100",
            "{body}",
        }
    }
}

/// # Errors
/// Returns error if formatting fails
pub fn edit_body(date: Date, text: Vec<StackString>, edit_button: bool) -> Result<String, Error> {
    let mut app = VirtualDom::new_with_props(
        EditElement,
        EditElementProps {
            date,
            text,
            edit_button,
        },
    );
    app.rebuild_in_place();
    let mut renderer = dioxus_ssr::Renderer::default();
    let mut buffer = String::new();
    renderer
        .render_to(&mut buffer, &app)
        .map_err(Into::<Error>::into)?;
    Ok(buffer)
}

#[component]
fn EditElement(date: Date, text: Vec<StackString>, edit_button: bool) -> Element {
    let text = text.join("\n");
    let buttons = if edit_button {
        rsx! {
            input {
                "type": "button",
                name: "edit",
                value: "Edit",
                "onclick": "switchToEditor('{date}')",
            }
        }
    } else {
        rsx! {
            form {
                id: "diary_edit_form",
                input {
                    "type": "button",
                    name: "update",
                    value: "Update",
                    "onclick": "submitFormData('{date}')",
                },
                input {
                    "type": "button",
                    name: "cancel",
                    value: "Cancel",
                    "onclick": "switchToDisplay('{date}')",
                }
            }
        }
    };
    let textarea = if edit_button {
        rsx! {
            textarea {
                name: "message",
                id: "diary_editor_form",
                rows: "50",
                cols: "100",
                form: "diary_edit_form",
                readonly: true,
                "{text}",
            }
        }
    } else {
        rsx! {
            textarea {
                name: "message",
                id: "diary_editor_form",
                rows: "50",
                cols: "100",
                form: "diary_edit_form",
                "{text}",
            }
        }
    };
    rsx! {
        {textarea},
        br {
            {buttons}
        }
    }
}

/// # Errors
/// Returns error if formatting fails
pub fn show_conflict_body(
    date: Date,
    conflicts: Vec<DiaryConflict>,
    datetime: DateTimeWrapper,
) -> Result<String, Error> {
    let mut app = VirtualDom::new_with_props(
        ShowConflictElement,
        ShowConflictElementProps {
            date,
            conflicts,
            datetime,
        },
    );
    app.rebuild_in_place();
    let mut renderer = dioxus_ssr::Renderer::default();
    let mut buffer = String::new();
    renderer
        .render_to(&mut buffer, &app)
        .map_err(Into::<Error>::into)?;
    Ok(buffer)
}

#[component]
fn ShowConflictElement(
    date: Date,
    conflicts: Vec<DiaryConflict>,
    datetime: DateTimeWrapper,
) -> Element {
    let conflict_text = {
        let diary_dates: BTreeSet<Date> = conflicts.iter().map(|entry| entry.diary_date).collect();
        if diary_dates.len() > 1 {
            Vec::new()
        } else {
            let date = diary_dates
                .into_iter()
                .next()
                .expect("Something has gone horribly wrong {datetime} {conflicts:?}");
            let conflicts: Vec<_> = conflicts
                .iter()
                .map(|entry| {
                    let nlines = entry.diff_text.split('\n').count() + 1;
                    let id = entry.id;
                    let diff = &entry.diff_text;
                    let dt = datetime
                        .format(format_description!(
                            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]Z"
                        ))
                        .unwrap_or_else(|_| String::new());
                    match entry.diff_type.as_ref() {
                        "rem" => rsx! {
                            textarea {
                                style: "color:Red;",
                                cols: 100,
                                rows: "{nlines}",
                                "{diff}"
                            },
                            div {
                                input {
                                    "type": "button",
                                    name: "add",
                                    value: "Add",
                                    "onclick": "updateConflictAdd('{id}', '{date}', '{dt}');",
                                }
                            }
                        },
                        "add" => rsx! {
                            textarea {
                                style: "color:Blue;",
                                cols: 100,
                                rows: "{nlines}",
                                "{diff}"
                            },
                            div {
                                input {
                                    "type": "button",
                                    name: "rm",
                                    value: "Rm",
                                    "onclick": "updateConflictRem('{id}', '{date}', '{dt}');",
                                }
                            }
                        },
                        _ => rsx! {
                            textarea {
                                cols: 100,
                                rows: "{nlines}",
                                "{diff}",
                            }
                        },
                    }
                })
                .collect();
            conflicts
        }
    };

    let dt = datetime
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]Z"
        ))
        .unwrap_or_else(|_| String::new());
    rsx! {
        div {
            {conflict_text.into_iter()},
        }
        input {
            "type": "button",
            name: "display",
            value: "Display",
            "onclick": "switchToDisplay('{date}')",
        },
        input {
            "type": "button",
            name: "commit",
            value: "Commit",
            "onclick": "commitConflict('{date}', '{dt}')",
        },
        input {
            "type": "button",
            name: "remove",
            value: "Remove",
            "onclick": "removeConflict('{date}', '{dt}')",
        },
        input {
            "type": "button",
            name: "edit",
            value: "Edit",
            "onclick": "switchToEditor('{date}')",
        },
    }
}
