use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use custom_callback::interaction::ViewerEvent;
use rerun::external::{eframe, re_crash_handler, re_grpc_server, re_log, re_memory, re_viewer};

#[global_allocator]
static GLOBAL: re_memory::AccountingAllocator<mimalloc::MiMalloc> =
    re_memory::AccountingAllocator::new(mimalloc::MiMalloc);

/// Minimum time between click events (debouncing)
const CLICK_DEBOUNCE_MS: u64 = 100;
/// Maximum rapid clicks to log as warning
const RAPID_CLICK_THRESHOLD: usize = 5;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let main_thread_token = re_viewer::MainThreadToken::i_promise_i_am_on_the_main_thread();
    re_log::setup_logging();
    re_crash_handler::install_crash_handlers(re_viewer::build_info());

    // Listen for gRPC connections from Rerun's logging SDKs.
    let rx_log = re_grpc_server::spawn_with_recv(
        "0.0.0.0:9877".parse()?,
        Default::default(),
        re_grpc_server::shutdown::never(),
    );

    // State for debouncing and rapid click detection
    let last_click_time = Rc::new(RefCell::new(Instant::now()));
    let rapid_click_count = Rc::new(RefCell::new(0usize));

    let mut native_options = re_viewer::native::eframe_options(None);
    native_options.viewport = native_options
        .viewport
        .with_app_id("rerun_example_custom_callback");

    let app_env = re_viewer::AppEnvironment::Custom("DimOS Interactive Viewer".to_owned());

    let startup_options = re_viewer::StartupOptions {
        on_event: Some(Rc::new({
            let last_click_time = last_click_time.clone();
            let rapid_click_count = rapid_click_count.clone();

            move |event: re_viewer::ViewerEvent| {
                if let re_viewer::ViewerEventKind::SelectionChange { items } = event.kind {
                    for item in items {
                        match item {
                            re_viewer::SelectionChangeItem::Entity {
                                entity_path,
                                view_name,
                                position: Some(pos),
                                ..
                            } => {
                                // Debouncing
                                let now = Instant::now();
                                let elapsed = now.duration_since(*last_click_time.borrow());

                                if elapsed < Duration::from_millis(CLICK_DEBOUNCE_MS) {
                                    let mut count = rapid_click_count.borrow_mut();
                                    *count += 1;
                                    if *count == RAPID_CLICK_THRESHOLD {
                                        re_log::warn!(
                                            "Rapid click detected ({} clicks within {}ms)",
                                            RAPID_CLICK_THRESHOLD,
                                            CLICK_DEBOUNCE_MS
                                        );
                                    }
                                    continue;
                                } else {
                                    *rapid_click_count.borrow_mut() = 0;
                                }
                                *last_click_time.borrow_mut() = now;

                                let timestamp_ms = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64;

                                let _click_event = ViewerEvent::Click {
                                    position: [pos.x, pos.y, pos.z],
                                    entity_path: Some(entity_path.to_string()),
                                    view_id: view_name.unwrap_or_else(|| "unknown_view".to_string()),
                                    timestamp_ms,
                                    is_2d: pos.z.abs() < 0.001,
                                };

                                // TODO: transport hook — PR #2 adds LCM publisher here
                                re_log::info!(
                                    "Click: entity={}, pos=({:.2}, {:.2}, {:.2})",
                                    entity_path,
                                    pos.x,
                                    pos.y,
                                    pos.z
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        })),
        ..Default::default()
    };

    let window_title = "DimOS Interactive Viewer";
    eframe::run_native(
        window_title,
        native_options,
        Box::new(move |cc| {
            re_viewer::customize_eframe_and_setup_renderer(cc)?;

            let mut rerun_app = re_viewer::App::new(
                main_thread_token,
                re_viewer::build_info(),
                app_env,
                startup_options,
                cc,
                None,
                re_viewer::AsyncRuntimeHandle::from_current_tokio_runtime_or_wasmbindgen()?,
            );

            rerun_app.add_log_receiver(rx_log);

            Ok(Box::new(rerun_app))
        }),
    )?;

    Ok(())
}
