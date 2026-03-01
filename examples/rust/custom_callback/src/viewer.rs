use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use custom_callback::comms::viewer::ControlViewer;
use custom_callback::interaction::{LcmPublisher, click_event_from_ms};
use custom_callback::panel::Control;
use rerun::external::{eframe, re_crash_handler, re_grpc_server, re_log, re_memory, re_viewer};

// By using `re_memory::AccountingAllocator` Rerun can keep track of exactly how much memory it is using,
// and prune the data store when it goes above a certain limit.
// By using `mimalloc` we get faster allocations.
#[global_allocator]
static GLOBAL: re_memory::AccountingAllocator<mimalloc::MiMalloc> =
    re_memory::AccountingAllocator::new(mimalloc::MiMalloc);

/// Port used for control messages (old protocol)
const CONTROL_PORT: u16 = 8889;
/// LCM channel for click events (follows RViz convention)
const LCM_CHANNEL: &str = "/clicked_point#geometry_msgs.PointStamped";
/// Minimum time between click events (debouncing)
const CLICK_DEBOUNCE_MS: u64 = 100;
/// Maximum rapid clicks to log as warning
const RAPID_CLICK_THRESHOLD: usize = 5;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let main_thread_token = re_viewer::MainThreadToken::i_promise_i_am_on_the_main_thread();
    // Direct calls using the `log` crate to stderr. Control with `RUST_LOG=debug` etc.
    re_log::setup_logging();

    // Install handlers for panics and crashes that prints to stderr and send
    // them to Rerun analytics (if the `analytics` feature is on in `Cargo.toml`).
    re_crash_handler::install_crash_handlers(re_viewer::build_info());

    // Listen for gRPC connections from Rerun's logging SDKs.
    let rx_log = re_grpc_server::spawn_with_recv(
        "0.0.0.0:9877".parse()?,
        Default::default(),
        re_grpc_server::shutdown::never(),
    );

    // Connect to the external application (old demo protocol on port 8889)
    let viewer = ControlViewer::connect(format!("127.0.0.1:{CONTROL_PORT}")).await?;
    let handle = viewer.handle();

    // Spawn the viewer client in a separate task
    tokio::spawn(async move {
        viewer.run().await;
    });

    // Create LCM publisher for click events (replaces TCP ViewerEventSender)
    let lcm_publisher = LcmPublisher::new(LCM_CHANNEL.to_string())
        .expect("Failed to create LCM publisher");
    re_log::info!("LCM publisher created for channel: {}", LCM_CHANNEL);

    // State for debouncing and rapid click detection
    let last_click_time = Rc::new(RefCell::new(Instant::now()));
    let rapid_click_count = Rc::new(RefCell::new(0usize));

    // Then we start the Rerun viewer
    let mut native_options = re_viewer::native::eframe_options(None);
    native_options.viewport = native_options
        .viewport
        .with_app_id("rerun_example_custom_callback");

    // This is used for analytics, if the `analytics` feature is on in `Cargo.toml`
    let app_env = re_viewer::AppEnvironment::Custom("My Custom Callback".to_owned());

    let startup_options = re_viewer::StartupOptions {
        on_event: Some(Rc::new({
            let last_click_time = last_click_time.clone();
            let rapid_click_count = rapid_click_count.clone();

            move |event: re_viewer::ViewerEvent| {
                // Handle selection changes with position data
                if let re_viewer::ViewerEventKind::SelectionChange { items } = event.kind {
                    let mut has_position = false;
                    let mut no_position_count = 0;

                    for item in items {
                        match item {
                            re_viewer::SelectionChangeItem::Entity {
                                entity_path,
                                view_name: _,
                                position: Some(pos),
                                ..
                            } => {
                                has_position = true;

                                // Debouncing: check time since last click
                                let now = Instant::now();
                                let elapsed = now.duration_since(*last_click_time.borrow());

                                if elapsed < Duration::from_millis(CLICK_DEBOUNCE_MS) {
                                    let mut count = rapid_click_count.borrow_mut();
                                    *count += 1;

                                    if *count == RAPID_CLICK_THRESHOLD {
                                        re_log::warn!(
                                            "Rapid click detected ({} clicks within {}ms). Events may be dropped.",
                                            RAPID_CLICK_THRESHOLD,
                                            CLICK_DEBOUNCE_MS
                                        );
                                    }

                                    // Skip this click event (debounced)
                                    continue;
                                } else {
                                    *rapid_click_count.borrow_mut() = 0;
                                }

                                *last_click_time.borrow_mut() = now;

                                // Get current timestamp
                                let timestamp_ms = SystemTime::now()
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_millis() as u64;

                                // Build click event and publish via LCM
                                let click = click_event_from_ms(
                                    [pos.x, pos.y, pos.z],
                                    &entity_path.to_string(),
                                    timestamp_ms,
                                );

                                match lcm_publisher.publish(&click) {
                                    Ok(_) => {
                                        re_log::debug!(
                                            "LCM click event published: entity={}, pos=({:.2}, {:.2}, {:.2})",
                                            entity_path,
                                            pos.x,
                                            pos.y,
                                            pos.z
                                        );
                                    }
                                    Err(err) => {
                                        re_log::error!("Failed to publish LCM click event: {:?}", err);
                                    }
                                }
                            }
                            re_viewer::SelectionChangeItem::Entity { position: None, .. } => {
                                no_position_count += 1;
                            }
                            _ => {}
                        }
                    }

                    if !has_position && no_position_count > 0 {
                        re_log::trace!(
                            "Selection change without position data ({} items). This is normal for hover/keyboard navigation.",
                            no_position_count
                        );
                    }
                }
            }
        })),
        ..Default::default()
    };

    let window_title = "Rerun Interactive Viewer";
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

            Ok(Box::new(Control::new(rerun_app, handle)))
        }),
    )?;

    Ok(())
}
