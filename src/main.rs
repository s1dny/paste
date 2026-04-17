use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use chrono::{DateTime, Duration, Utc};
use rand::prelude::IndexedRandom;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

static WORDLIST: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| include_str!("../assets/wordlist.txt").lines().collect());
const MAX_PASTE_BYTES: usize = 1024 * 1024;
const MAX_REQUEST_BYTES: usize = MAX_PASTE_BYTES * 4;

struct AppState {
    pastes: RwLock<HashMap<String, Paste>>,
}

#[derive(Clone)]
struct Paste {
    content: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    burn_after_read: bool,
}

#[derive(Deserialize)]
struct PasteForm {
    content: String,
    #[serde(default)]
    burn_after_read: Option<String>,
}

#[derive(Serialize)]
struct ApiResponse {
    id: String,
    url: String,
}

#[derive(Serialize)]
struct OpenPasteResponse {
    content: String,
}

#[derive(Deserialize, Default)]
struct ApiCreateParams {
    #[serde(default)]
    burn_after_read: Option<String>,
}

#[tokio::main]
async fn main() {
    let app_state = Arc::new(AppState {
        pastes: RwLock::new(HashMap::new()),
    });

    let cleanup_state = app_state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            interval.tick().await;
            cleanup_expired_pastes(&cleanup_state);
        }
    });

    let app = Router::new()
        .route("/", get(home_handler))
        .route("/paste", post(create_paste_handler))
        .route("/{id}", get(view_paste_handler))
        .route("/{id}/open", post(open_paste_handler))
        .route("/{id}/raw", get(raw_paste_handler))
        .route("/api/paste", post(api_create_paste_handler))
        .nest_service("/static", ServeDir::new("static"))
        .layer(DefaultBodyLimit::max(MAX_REQUEST_BYTES))
        .with_state(app_state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn home_handler() -> Html<String> {
    render_home(None)
}

fn render_home(error_message: Option<&str>) -> Html<String> {
    let error_html = error_message
        .map(|message| format!(r#"<p class="error-banner">{}</p>"#, html_escape(message)))
        .unwrap_or_default();

    Html(include_str!("../static/index.html").replace("{{ERROR_MESSAGE}}", &error_html))
}

fn insert_paste(state: &AppState, content: String, burn_after_read: bool) -> String {
    let now = Utc::now();
    let paste = Paste {
        content,
        created_at: now,
        expires_at: now + Duration::hours(24),
        burn_after_read,
    };

    let mut pastes = state.pastes.write().unwrap();
    let id = generate_unique_id(&pastes);
    pastes.insert(id.clone(), paste);
    id
}

async fn create_paste_handler(
    State(state): State<Arc<AppState>>,
    Form(form): Form<PasteForm>,
) -> Response {
    match validate_paste_content(&form.content) {
        Ok(()) => {
            let id = insert_paste(&state, form.content, form.burn_after_read.is_some());
            Redirect::to(&format!("/{}", id)).into_response()
        }
        Err((status, message)) => (status, render_home(Some(message))).into_response(),
    }
}

async fn api_create_paste_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ApiCreateParams>,
    body: String,
) -> impl IntoResponse {
    if let Err((status, message)) = validate_paste_content(&body) {
        return (status, Json(serde_json::json!({ "error": message }))).into_response();
    }

    let id = insert_paste(&state, body, params.burn_after_read.is_some());
    (
        StatusCode::CREATED,
        Json(ApiResponse {
            url: format!("/{}", id),
            id,
        }),
    )
        .into_response()
}

async fn view_paste_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match get_paste(&state, &id) {
        Some(paste) => {
            let creation_time_str = paste.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
            let burn_after_read = paste.burn_after_read;
            let paste_content = if burn_after_read {
                String::new()
            } else {
                html_escape(&paste.content)
            };
            let burn_gate = if burn_after_read {
                r#"
        <section id="burn-gate" class="burn-gate">
            <button id="open-burn-btn" class="btn">open and burn</button>
        </section>
"#
                .to_string()
            } else {
                String::new()
            };
            let raw_button_hidden = if burn_after_read { " hidden" } else { "" };
            let copy_button_disabled = if burn_after_read { " disabled" } else { "" };
            let paste_size_html = if burn_after_read {
                String::new()
            } else {
                format!("<span>{}</span>", format_size(paste.content.len()))
            };

            let html = include_str!("../static/view.html")
                .replace("{{PASTE_ID}}", &id)
                .replace("{{CREATION_TIME}}", &creation_time_str)
                .replace("{{PASTE_SIZE_HTML}}", &paste_size_html)
                .replace("{{PASTE_CONTENT}}", &paste_content)
                .replace("{{BURN_GATE}}", &burn_gate)
                .replace("{{RAW_BUTTON_HIDDEN}}", raw_button_hidden)
                .replace("{{COPY_BUTTON_DISABLED}}", copy_button_disabled)
                .replace(
                    "{{BURN_AFTER_READ}}",
                    if burn_after_read { "true" } else { "false" },
                );
            Html(html).into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            String::from("paste not found or expired"),
        )
            .into_response(),
    }
}

async fn raw_paste_handler(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    match take_paste_for_read(&state, &id) {
        Some(paste) => (
            StatusCode::OK,
            [("Content-Type", "text/plain; charset=utf-8")],
            paste.content,
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            [("Content-Type", "text/plain; charset=utf-8")],
            "paste not found or expired.".to_string(),
        )
            .into_response(),
    }
}

async fn open_paste_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match take_paste_for_read(&state, &id) {
        Some(paste) => (
            StatusCode::OK,
            Json(OpenPasteResponse {
                content: paste.content,
            }),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "paste not found or expired" })),
        )
            .into_response(),
    }
}

fn get_paste(state: &AppState, id: &str) -> Option<Paste> {
    let now = Utc::now();
    let mut pastes = state.pastes.write().unwrap();
    let paste = pastes.get(id)?.clone();

    if paste.expires_at <= now {
        pastes.remove(id);
        return None;
    }

    Some(paste)
}

fn take_paste_for_read(state: &AppState, id: &str) -> Option<Paste> {
    let now = Utc::now();
    let mut pastes = state.pastes.write().unwrap();
    let paste = pastes.get(id)?.clone();

    if paste.expires_at <= now {
        pastes.remove(id);
        return None;
    }

    if paste.burn_after_read {
        pastes.remove(id);
    }

    Some(paste)
}

fn validate_paste_content(content: &str) -> Result<(), (StatusCode, &'static str)> {
    if content.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "content must not be empty"));
    }

    if content.len() > MAX_PASTE_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "paste must be 1.0MB or smaller",
        ));
    }

    Ok(())
}

fn generate_unique_id(pastes: &HashMap<String, Paste>) -> String {
    let mut rng = rand::rng();
    loop {
        let word1 = WORDLIST.choose(&mut rng).unwrap();
        let word2 = WORDLIST.choose(&mut rng).unwrap();
        let id = format!("{}.{}", word1, word2);
        if !pastes.contains_key(&id) {
            return id;
        }
    }
}

fn cleanup_expired_pastes(state: &AppState) {
    let now = Utc::now();
    state
        .pastes
        .write()
        .unwrap()
        .retain(|_, paste| paste.expires_at > now);
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_size(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let size = bytes as f64;

    if size < KB {
        format!("{:.1}B", size)
    } else if size < MB {
        format!("{:.1}KB", size / KB)
    } else if size < GB {
        format!("{:.1}MB", size / MB)
    } else {
        format!("{:.1}GB", size / GB)
    }
}
