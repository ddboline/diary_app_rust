use dioxus::prelude::{
    dioxus_elements, inline_props, rsx, Element, GlobalAttributes, Props, Scope, VirtualDom,
};
use rweb_helper::DateType;
use stack_string::StackString;
use std::collections::HashSet;
use time::{macros::format_description, Date, OffsetDateTime};
use time_tz::OffsetDateTimeExt;

use diary_app_lib::{date_time_wrapper::DateTimeWrapper, models::DiaryConflict};

#[inline_props]
fn conflict_element(
    cx: Scope,
    conflicts: Vec<DiaryConflict>,
    datetime: DateTimeWrapper,
    date: Date,
) -> Element {
    cx.render(rsx! {
        conflicts.iter().enumerate().map(|(idx, entry)| {
            let nlines = entry.diff_text.split('\n').count() + 1;
            let id = entry.id;
            let diff = &entry.diff_text;
            let dt = datetime.format(format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]Z")).unwrap_or_else(|_| String::new());
            match entry.diff_type.as_ref() {
                "rem" => rsx! {
                    textarea {
                        key: "rem-key-{idx}",
                        "style": "color:Red;",
                        cols: "100",
                        rows: "{nlines}",
                        "{diff}",
                    },
                    input {
                        key: "add-key-{idx}",
                        "type": "button",
                        name: "add",
                        value: "Add",
                        "onclick": "updateConflictAdd('{id}', '{date}', '{dt}');",
                    }
                },
                "add" => rsx! {
                    textarea {
                        key: "add-key-{idx}",
                        "style": "color:Blue;",
                        cols: "100",
                        rows: "{nlines}",
                        "{diff}",
                    },
                    input {
                        key: "rem-key-{idx}",
                        "type": "button",
                        name: "rm",
                        value: "Rm",
                        "onclick": "updateConflictRem('{id}', '{date}', '{dt}');",
                    }
                },
                _ => rsx! {
                    textarea {
                        key: "diff-key-{idx}",
                        cols: "100",
                        rows: "{nlines}",
                        "{diff}",
                    }
                }
            }
        })
    })
}

pub fn index_body() -> String {
    let mut app = VirtualDom::new(index_element);
    drop(app.rebuild());
    dioxus_ssr::render(&app)
}

fn index_element(cx: Scope) -> Element {
    cx.render(rsx! {
        head {
            style {include_str!("../../templates/style.css")}
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
                    "&nbsp;",
                },
                "<br>",
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
                include_str!("../../templates/scripts.js")
            }
        }
    })
}

pub fn list_body(
    conflicts: HashSet<DateType>,
    dates: Vec<DateType>,
    start: Option<usize>,
) -> String {
    let mut app = VirtualDom::new_with_props(
        date_list_element,
        date_list_elementProps {
            conflicts,
            dates,
            start,
        },
    );
    drop(app.rebuild());
    dioxus_ssr::render(&app)
}

#[inline_props]
fn date_list_element(
    cx: Scope,
    conflicts: HashSet<DateType>,
    dates: Vec<DateType>,
    start: Option<usize>,
) -> Element {
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
    cx.render(rsx! {
        dates.iter().enumerate().map(|(idx, t)| {
            let d: Date = (*t).into();
            let c = if conflicts.contains(t) {
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
                        c
                    },
                    "<br>",
                }
            }
        })
        buttons,
    })
}

pub fn list_conflicts_body(date: Option<DateType>, conflicts: Vec<DateTimeWrapper>) -> String {
    let mut app = VirtualDom::new_with_props(
        list_conflicts_element,
        list_conflicts_elementProps { date, conflicts },
    );
    drop(app.rebuild());
    dioxus_ssr::render(&app)
}

#[inline_props]
fn list_conflicts_element(
    cx: Scope,
    date: Option<DateType>,
    conflicts: Vec<DateTimeWrapper>,
) -> Element {
    let local = DateTimeWrapper::local_tz();
    let clean_conflicts = if let Some(date) = date {
        if conflicts.is_empty() {
            None
        } else {
            let date: Date = (*date).into();
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
    cx.render(rsx! {
        conflicts.iter().enumerate().map(|(idx, t)| {
            let d: Date = date.unwrap_or_else(|| OffsetDateTime::now_utc().to_timezone(local).date().into()).into();
            rsx! {
                input {
                    key: "show-key-{idx}",
                    "type": "button",
                    name: "show_{t}",
                    value: "Show {t}",
                    "onclick": "showConflict( '{d}', '{t}' )",
                    "<br>",
                }
            }
        }),
        "<br>",
        clean_conflicts,
        button {
            "type": "submit",
            "onclick": "switchToList()",
            "List",
        },
        "<br>"
    })
}

pub fn search_body(results: Vec<StackString>) -> String {
    let mut app = VirtualDom::new_with_props(search_element, search_elementProps { results });
    drop(app.rebuild());
    dioxus_ssr::render(&app)
}

#[inline_props]
fn search_element(cx: Scope, results: Vec<StackString>) -> Element {
    let body = results.join("\n");
    cx.render(rsx! {
        textarea {
            "autofocus": "true",
            readonly: "readonly",
            name: "message",
            id: "diary_editor_form",
            "rows": "50",
            "cols": "100",
            "{body}",
        }
    })
}

pub fn edit_body(date: Date, text: Vec<StackString>, do_update: bool) -> String {
    let mut app = VirtualDom::new_with_props(
        edit_element,
        edit_elementProps {
            date,
            text,
            do_update,
        },
    );
    drop(app.rebuild());
    dioxus_ssr::render(&app)
}

#[inline_props]
fn edit_element(cx: Scope, date: Date, text: Vec<StackString>, do_update: bool) -> Element {
    let text = text.join("\n");
    let buttons = if *do_update {
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
    cx.render(rsx! {
        textarea {
            name: "message",
            id: "diary_editor_form",
            rows: "50",
            cols: "100",
            form: "diary_edit_form",
            "{text}",
        },
        "<br>",
        buttons,
    })
}

pub fn show_conflict_body(date: Date, text: Vec<StackString>, datetime: DateTimeWrapper) -> String {
    let mut app = VirtualDom::new_with_props(
        show_conflict_element,
        show_conflict_elementProps {
            date,
            text,
            datetime,
        },
    );
    drop(app.rebuild());
    dioxus_ssr::render(&app)
}

#[inline_props]
fn show_conflict_element(
    cx: Scope,
    date: Date,
    text: Vec<StackString>,
    datetime: DateTimeWrapper,
) -> Element {
    let text = text.join("\n");
    let dt = datetime
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond]Z"
        ))
        .unwrap_or_else(|_| String::new());
    cx.render(rsx! {
        br {
            "{text}",
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
    })
}
