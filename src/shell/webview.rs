use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::json;
use tao::event_loop::EventLoopProxy;
use wry::http::{Request, Response};
use wry::WebViewBuilder;

use crate::bridge::router::CommandRouter;
use crate::bridge::EventSink;
use crate::core::app_log::AppLog;
use crate::core::app_paths::AppPaths;
use crate::core::win32;

pub const PROTOCOL: &str = "app";
pub const ENTRY_URL: &str = "http://app.localhost/index.html";
const ASSET_ROOT_DIRECTORY: &str = "wwwroot";

pub const BRIDGE_SHIM: &str = r#"(function () {
  var listeners = [];
  window.__hostPush = function (text) {
    var data = typeof text === "string" ? JSON.parse(text) : text;
    for (var index = 0; index < listeners.length; index += 1) {
      try { listeners[index]({ data: data }); } catch (error) { }
    }
  };
  var chromeRef = window.chrome || {};
  var nativeWebview = chromeRef.webview;
  var nativePost = nativeWebview && typeof nativeWebview.postMessage === "function"
    ? nativeWebview.postMessage.bind(nativeWebview)
    : null;
  window.chrome = chromeRef;
  window.chrome.webview = {
    postMessage: function (message) {
      if (!nativePost) throw new Error("bridge_unavailable");
      nativePost(typeof message === "string" ? message : JSON.stringify(message));
    },
    addEventListener: function (type, handler) {
      if (type === "message" && typeof handler === "function") listeners.push(handler);
    },
    removeEventListener: function (type, handler) {
      var index = listeners.indexOf(handler);
      if (index >= 0) listeners.splice(index, 1);
    }
  };
})();"#;

pub fn data_directory() -> PathBuf {
    AppPaths::webview_data_directory()
}

pub fn asset_root() -> PathBuf {
    AppPaths::executable_directory().join(ASSET_ROOT_DIRECTORY)
}

pub fn assets_available() -> bool {
    asset_root().join("index.html").is_file()
}

pub fn attach(
    window: &tao::window::Window,
    proxy: EventLoopProxy<UserEvent>,
    router: &Arc<CommandRouter>,
) -> Result<(wry::WebView, wry::WebContext), String> {
    let root = asset_root();
    let navigation_proxy = proxy.clone();
    let ipc_proxy = proxy.clone();
    let router = Arc::clone(router);
    let root_for_protocol = root.clone();
    let mut context = wry::WebContext::new(Some(AppPaths::webview_data_directory()));
    let webview = WebViewBuilder::with_web_context(&mut context)
        .with_url(ENTRY_URL)
        .with_initialization_script(BRIDGE_SHIM)
        .with_custom_protocol(PROTOCOL.to_string(), move |_id, request| {
            respond(&root_for_protocol, request)
        })
        .with_navigation_handler(move |url| {
            if is_internal(&url) {
                return true;
            }
            let _ = navigation_proxy.send_event(UserEvent::OpenExternal(url));
            false
        })
        .with_ipc_handler(move |request| {
            let _ = ipc_proxy.send_event(UserEvent::Message(request.body().clone()));
        })
        .with_devtools(false)
        .build(window)
        .map_err(|error| error.to_string())?;
    router.attach_window(window);
    Ok((webview, context))
}

pub enum UserEvent {
    Message(String),
    Push(String),
    OpenExternal(String),
}

pub fn event_sink(proxy: EventLoopProxy<UserEvent>) -> EventSink {
    Arc::new(move |payload| {
        let text = serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_string());
        let _ = proxy.send_event(UserEvent::Push(text));
    })
}

pub fn error_page(title: &str, body: &str) -> String {
    let escape = |value: &str| {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
    };
    format!(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\">\
         <title>{}</title><style>\
         html,body{{margin:0;height:100%;background:#f4f7f6;color:#0f1c1a;\
         font-family:'Segoe UI',system-ui,sans-serif}}\
         main{{max-width:560px;margin:0 auto;padding:56px 24px}}\
         section{{background:#fff;border:1px solid #dae2e0;border-radius:10px;padding:28px}}\
         h1{{margin:0 0 12px;font-size:20px}}p{{margin:0;font-size:13px;line-height:1.6}}\
         small{{display:block;margin-top:14px;color:#6b7a77;font-size:11px}}\
         </style></head><body><main><section><h1>{}</h1><p>{}</p>\
         <small>{}</small></section></main></body></html>",
        escape(title),
        escape(title),
        escape(body),
        escape(&AppPaths::log_path().to_string_lossy())
    )
}

pub fn open_external(url: &str) {
    if crate::core::network::validate_https(url).is_err() {
        AppLog::warn(&format!("Blocked external navigation: {url}"));
        return;
    }
    if let Err(error) = win32::open_external(url) {
        AppLog::warn(&error);
    }
}

pub fn handle_external(url: &str) {
    match crate::core::network::validate_https(url) {
        Ok(_) => open_external(url),
        Err(error) => AppLog::warn(&error.message()),
    }
}

fn is_internal(url: &str) -> bool {
    let lowered = url.to_lowercase();
    lowered.starts_with("http://app.localhost/")
        || lowered.starts_with("https://app.localhost/")
        || lowered.starts_with("app://")
        || lowered.starts_with("http://app.localhost")
        || lowered.starts_with("about:blank")
}

fn request_path(uri: &wry::http::Uri) -> String {
    let raw = uri.to_string();
    let authority = raw.find("://").map(|index| index + 3).unwrap_or(0);
    let after = &raw[authority..];
    let start = after.find('/').unwrap_or(after.len());
    let path = &after[start..];
    let path = match path.find(['?', '#']) {
        Some(index) => &path[..index],
        None => path,
    };
    let trimmed = path.trim_start_matches('/');
    if trimmed.is_empty() {
        "index.html".to_string()
    } else {
        trimmed.to_string()
    }
}

fn respond(root: &Path, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    let relative = request_path(request.uri());
    let not_found = || {
        Response::builder()
            .status(404)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Cow::Owned(b"Not found".to_vec()))
            .unwrap_or_else(|_| Response::new(Cow::Owned(Vec::new())))
    };
    if relative.contains("..") {
        return not_found();
    }
    let target = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !target.is_file() {
        return not_found();
    }
    let Ok(bytes) = std::fs::read(&target) else {
        return not_found();
    };
    Response::builder()
        .status(200)
        .header("Content-Type", content_type(&target))
        .header("Cache-Control", "no-store")
        .body(Cow::Owned(bytes))
        .unwrap_or_else(|_| not_found())
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default()
        .as_str()
    {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub fn push_script(text: &str) -> String {
    let payload = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".to_string());
    format!("window.__hostPush({payload});")
}

pub fn response_script(
    id: i64,
    ok: bool,
    result: serde_json::Value,
    error: Option<String>,
) -> String {
    let payload = json!({ "id": id, "ok": ok, "result": result, "error": error }).to_string();
    push_script(&payload)
}
