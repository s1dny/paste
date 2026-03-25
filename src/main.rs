use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use chrono::{DateTime, Duration, Utc};
use rand::prelude::IndexedRandom;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
};
use tokio::net::TcpListener;
use tower_http::services::ServeDir;

static WORDLIST: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| include_str!("../wordlist.txt").lines().collect());

struct AppState {
    pastes: RwLock<HashMap<String, Paste>>,
}

#[derive(Clone)]
struct Paste {
    content: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct PasteForm {
    content: String,
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
        .route("/{id}/raw", get(raw_paste_handler))
        .nest_service("/static", ServeDir::new("static"))
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .with_state(app_state);

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}

async fn home_handler() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn create_paste_handler(
    State(state): State<Arc<AppState>>,
    Form(form): Form<PasteForm>,
) -> impl IntoResponse {
    let now = Utc::now();
    let paste = Paste {
        content: form.content,
        created_at: now,
        expires_at: now + Duration::hours(24),
    };

    let id = {
        let mut pastes = state.pastes.write().unwrap();
        let id = generate_unique_id(&pastes);
        pastes.insert(id.clone(), paste);
        id
    };

    Redirect::to(&format!("/{}", id))
}

async fn view_paste_handler(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    match get_paste(&state, &id) {
        Some(paste) => {
            let creation_time_str = paste.created_at.format("%Y-%m-%d %H:%M:%S").to_string();
            let paste_size_str = format_size(paste.content.len());

            let html = include_str!("../static/view.html")
                .replace("{{PASTE_ID}}", &id)
                .replace("{{CREATION_TIME}}", &creation_time_str)
                .replace("{{PASTE_SIZE}}", &paste_size_str)
                .replace("{{PASTE_CONTENT}}", &html_escape(&paste.content));
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
    match get_paste(&state, &id) {
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

fn get_paste(state: &AppState, id: &str) -> Option<Paste> {
    let pastes = state.pastes.read().unwrap();
    pastes
        .get(id)
        .filter(|paste| paste.expires_at > Utc::now())
        .cloned()
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
