!function() {
    gotoEntries( 0 );
}();
var autosave_timeout = null;
function updateMainArticle( url , nav_update=null, status="done", method="GET" ) {
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("diary_status").innerHTML = status;
        document.getElementById("main_article").innerHTML = xmlhttp.responseText;
        addTabEventHandler();
        if (nav_update) {
            nav_update();
        } else {
            gotoEntries(0);
        }
        setTextAreaRowsCols();
    }
    xmlhttp.open(method, url, true);
    xmlhttp.send(null);
}
function setTextAreaRowsCols() {
    let textarea = document.getElementById('diary_editor_form');
    if (textarea) {
        if (textarea.getAttribute('rows')) {
            textarea.setAttribute('rows', Math.floor(window.innerHeight * 37 / 856.));
        }
        if (textarea.getAttribute('cols')) {
            textarea.setAttribute('cols', Math.floor(window.innerWidth * 90 / 1105.));
        }
    }
}
function switchToDate( date ) {
    if (autosave_timeout) {
        clearInterval(autosave_timeout);
    }
    updateMainArticle('../api/display?date=' + date, null, status=date)
}
function listConflicts( date ) {
    let url = '../api/list_conflicts?date=' + date;
    updateNavigation(url);
}
function showConflict( date, datetime ) {
    let url = '../api/show_conflict?date=' + date + '&datetime=' + datetime;
    updateMainArticle(url, () => listConflicts(date), status=date);
}
function cleanConflicts(date) {
    let url = '../api/remove_conflict?date=' + date;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('DELETE', url, true);
    xmlhttp.onload = function see_result() {
        switchToDate( date )
    }
    xmlhttp.send(null);
}
function removeConflict( date, datetime ) {
    let url = '../api/remove_conflict?datetime=' + datetime;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('DELETE', url, true);
    xmlhttp.onload = function see_result() {
        switchToDate( date )
    }
    xmlhttp.send(null);
}
function commitConflict( date, datetime ) {
    let url = '../api/commit_conflict?datetime=' + datetime;
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('POST', url, true);
    xmlhttp.onload = function see_result() {
        removeConflict( date, datetime );
    }
    xmlhttp.send(null);
}
function searchDiary() {
    let text_form = document.getElementById( 'search_text' );
    let url = encodeURI('../api/search?text=' + text_form.value);
    updateMainArticle(url, null, status=text_form.value);
}
function searchDate() {
    let text_form = document.getElementById( 'search_date' );
    switchToDate( text_form.value );
}
function syncDiary() {
    updateMainArticle('../api/sync', method="POST");
    document.getElementById("main_article").innerHTML = "syncing..."
}
function updateNavigation( url ) {
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.onload = function f() {
        document.getElementById("navigation").innerHTML = xmlhttp.responseText;
    }
    xmlhttp.open("GET", url, true);
    xmlhttp.send(null);
}
function gotoEntries( increment ) {
    increment = Number(increment);
    let start = Number(document.getElementById('navigation').getAttribute('start'));
    start = start + increment;
    let url = '../api/list';
    if (start > 0) {
        url = url + '?start=' + start + '&limit=10';
        document.getElementById('navigation').setAttribute('start', start);
    } else {
        url = url + '?limit=10';
        document.getElementById('navigation').setAttribute('start', 0);
    }
    updateNavigation(url);
}
function switchToList() {
    location.replace('../api/index.html');
}
function submitFormData( date ) {
    let url = '../api/replace';
    let text = document.getElementById( 'diary_editor_form' );
    let data = JSON.stringify({'date': date, 'text': text.value});
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('POST', url, true);
    xmlhttp.onload = function see_result() {
        switchToDate( date );
    }
    xmlhttp.setRequestHeader('Content-Type', 'application/json');
    xmlhttp.send(data);
}
function switchToDisplay( date ) {
    switchToDate( date );
}
function autoSave( date ) {
    let url = '../api/replace';
    let text = document.getElementById( 'diary_editor_form' );
    let data = JSON.stringify({'date': date, 'text': text.value});
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('POST', url, true);
    xmlhttp.setRequestHeader('Content-Type', 'application/json');
    xmlhttp.send(data);
}
function switchToEditor( date ) {
    let url = '../api/edit?date=' + date;
    updateMainArticle(url, null, status=date);
    autosave_timeout = setInterval(function() {
        autoSave(date)
    }, 60000);
}
function updateConflictAdd( id, date, datetime ) {
    let url = '../api/update_conflict?id=' + id + '&diff_type=add';
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('PATCH', url, true);
    xmlhttp.onload = function see_result() {
        showConflict( date, datetime );
    }
    xmlhttp.send(null);
}
function updateConflictRem( id, date, datetime ) {
    let url = '../api/update_conflict?id=' + id + '&diff_type=rem';
    let xmlhttp = new XMLHttpRequest();
    xmlhttp.open('PATCH', url, true);
    xmlhttp.onload = function see_result() {
        showConflict( date, datetime );
    }
    xmlhttp.send(null);
}
function addTabEventHandler() {
    let editor_form = document.getElementById( 'diary_editor_form' );
    if (editor_form) {
        editor_form.addEventListener(
            'keydown',
            function(e) {
                if(e.keyCode === 9) { // tab was pressed
                    // get caret position/selection
                    let start = this.selectionStart;
                    let end = this.selectionEnd;

                    let target = e.target;
                    let value = target.value;

                    // set textarea value to: text before caret + tab + text after caret
                    target.value = value.substring(0, start)
                                + "    "
                                + value.substring(end);

                    // put caret at right position again (add one for the tab)
                    this.selectionStart = this.selectionEnd = start + 4;

                    // prevent the focus lose
                    e.preventDefault();
                }
            },
            false
        );
    }
}
