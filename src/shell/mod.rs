pub mod uninstall;
pub mod webview;

use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Icon, WindowBuilder};

use crate::bridge::router::CommandRouter;
use crate::core::app_log::AppLog;
use crate::core::win32;

const WINDOW_TITLE: &str = "Whisper Speech Recognition";
const WINDOW_WIDTH: f64 = 1180.0;
const WINDOW_HEIGHT: f64 = 780.0;
const WINDOW_MIN_WIDTH: f64 = 720.0;
const WINDOW_MIN_HEIGHT: f64 = 600.0;

pub fn run() -> i32 {
    if !webview::assets_available() {
        let message = "Interface resources are missing or unreadable. Reinstall the application and try again.";
        AppLog::error(message);
        win32::show_error(WINDOW_TITLE, message);
        return 1;
    }

    let mut builder = EventLoopBuilder::<webview::UserEvent>::with_user_event();
    let event_loop = builder.build();
    let proxy = event_loop.create_proxy();
    let sink = webview::event_sink(proxy.clone());
    let router = CommandRouter::new(sink);

    let window = match build_window(&event_loop) {
        Ok(value) => value,
        Err(error) => {
            AppLog::error(&error);
            win32::show_error(WINDOW_TITLE, &error);
            return 1;
        }
    };

    let (webview, _context) = match webview::attach(&window, proxy, &router) {
        Ok(value) => value,
        Err(error) => {
            AppLog::error(&error);
            win32::show_error(
                WINDOW_TITLE,
                "Microsoft Edge WebView2 Runtime is unavailable. Install it and reopen the application.",
            );
            return 1;
        }
    };

    event_loop.run(move |event, _target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::ExitWithCode(0);
            }
            Event::UserEvent(webview::UserEvent::Message(body)) => {
                router.handle_message(body);
            }
            Event::UserEvent(webview::UserEvent::Push(text)) => {
                let _ = webview.evaluate_script(&webview::push_script(&text));
            }
            Event::UserEvent(webview::UserEvent::OpenExternal(url)) => {
                webview::handle_external(&url);
            }
            _ => {}
        }
    });
}

fn build_window(
    event_loop: &tao::event_loop::EventLoop<webview::UserEvent>,
) -> Result<tao::window::Window, String> {
    let mut builder = WindowBuilder::new()
        .with_title(WINDOW_TITLE)
        .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .with_min_inner_size(LogicalSize::new(WINDOW_MIN_WIDTH, WINDOW_MIN_HEIGHT))
        .with_resizable(true);
    if let Some(icon) = load_icon() {
        builder = builder.with_window_icon(Some(icon));
    }
    builder.build(event_loop).map_err(|error| error.to_string())
}

fn load_icon() -> Option<Icon> {
    let path = crate::core::app_paths::executable_directory().join("app.ico");
    let bytes = std::fs::read(path).ok()?;
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Ico).ok()?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    Icon::from_rgba(rgba.into_raw(), width, height).ok()
}
