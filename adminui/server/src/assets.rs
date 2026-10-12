//! The web app, built by Vite into ../web/dist and embedded. Pre-compressed
//! `.br`/`.gz` siblings are served when the browser accepts them.

use axum::body::Body;
use axum::http::{header, HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

#[derive(rust_embed::RustEmbed)]
#[folder = "../web/dist/"]
struct Web;

fn mime(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "json" | "webmanifest" => "application/json",
        "woff2" => "font/woff2",
        _ => "application/octet-stream",
    }
}

pub async fn serve(uri: Uri, h: HeaderMap) -> Response {
    let mut path = uri.path().trim_start_matches('/').to_string();
    if path.is_empty() || Web::get(&path).is_none() && !path.contains('.') {
        path = "index.html".into(); // SPA fallback
    }
    let accept = h.get(header::ACCEPT_ENCODING).and_then(|v| v.to_str().ok()).unwrap_or("");
    let (file, enc) = if accept.contains("br") && Web::get(&format!("{path}.br")).is_some() {
        (Web::get(&format!("{path}.br")), Some("br"))
    } else if accept.contains("gzip") && Web::get(&format!("{path}.gz")).is_some() {
        (Web::get(&format!("{path}.gz")), Some("gzip"))
    } else {
        (Web::get(&path), None)
    };
    let Some(f) = file else {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    };
    let cache = if path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    let mut b = Response::builder()
        .header(header::CONTENT_TYPE, mime(&path))
        .header(header::CACHE_CONTROL, cache)
        .header(header::VARY, "Accept-Encoding")
        .header("X-Content-Type-Options", "nosniff")
        .header("X-Frame-Options", "DENY")
        .header("Referrer-Policy", "same-origin");
    if path == "index.html" {
        b = b.header(
            "Content-Security-Policy",
            "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
        );
    }
    if let Some(e) = enc {
        b = b.header(header::CONTENT_ENCODING, e);
    }
    b.body(Body::from(f.data.into_owned())).unwrap()
}
