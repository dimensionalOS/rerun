use tokio::sync::mpsc;
use super::protocol::ViewerEvent;

/// Handle for sending interaction events from the viewer to the application.
/// 
/// This is designed to be cheap to clone and thread-safe, so it can be embedded
/// in ViewerContext and shared across all views and UI components.
#[derive(Clone)]
pub struct InteractionHandle {
    tx: mpsc::UnboundedSender<ViewerEvent>,
}

impl InteractionHandle {
    /// Create a new handle from a channel sender.
    pub fn new(tx: mpsc::UnboundedSender<ViewerEvent>) -> Self {
        Self { tx }
    }
    
    /// Send a click event to the application.
    pub fn send_click(
        &self,
        position: [f32; 3],
        entity_path: Option<String>,
        view_id: String,
        is_2d: bool,
    ) {
        let event = ViewerEvent::Click {
            position,
            entity_path,
            view_id,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            is_2d,
        };
        
        // Log if send fails, but don't panic
        if let Err(e) = self.tx.send(event) {
            eprintln!("Failed to send click event: {}", e);
        }
    }
    
    /// Send a waypoint completion event.
    pub fn send_waypoint_complete(&self, waypoints: Vec<[f32; 3]>) {
        let event = ViewerEvent::WaypointComplete { waypoints };
        
        if let Err(e) = self.tx.send(event) {
            eprintln!("Failed to send waypoint complete event: {}", e);
        }
    }
    
    /// Send a mode changed event.
    pub fn send_mode_changed(&self, mode: String) {
        let event = ViewerEvent::ModeChanged { mode };
        
        if let Err(e) = self.tx.send(event) {
            eprintln!("Failed to send mode changed event: {}", e);
        }
    }
    
    /// Send a disconnect event.
    pub fn send_disconnect(&self) {
        let event = ViewerEvent::Disconnect;
        
        let _ = self.tx.send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_handle_send_click() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = InteractionHandle::new(tx);
        
        handle.send_click(
            [1.0, 2.0, 3.0],
            Some("world/robot".to_string()),
            "view_123".to_string(),
            false,
        );
        
        let event = rx.try_recv().unwrap();
        match event {
            ViewerEvent::Click { position, entity_path, view_id, is_2d, .. } => {
                assert_eq!(position, [1.0, 2.0, 3.0]);
                assert_eq!(entity_path, Some("world/robot".to_string()));
                assert_eq!(view_id, "view_123");
                assert!(!is_2d);
            }
            _ => panic!("Expected Click event"),
        }
    }
    
    #[test]
    fn test_handle_send_waypoint_complete() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = InteractionHandle::new(tx);
        
        let waypoints = vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
        handle.send_waypoint_complete(waypoints.clone());
        
        let event = rx.try_recv().unwrap();
        match event {
            ViewerEvent::WaypointComplete { waypoints: w } => {
                assert_eq!(w, waypoints);
            }
            _ => panic!("Expected WaypointComplete event"),
        }
    }
    
    #[test]
    fn test_handle_is_cloneable() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle1 = InteractionHandle::new(tx);
        let handle2 = handle1.clone();
        
        // Both handles should work
        handle1.send_mode_changed("click".to_string());
        handle2.send_mode_changed("waypoint".to_string());
        
        let event1 = rx.try_recv().unwrap();
        let event2 = rx.try_recv().unwrap();
        
        assert!(matches!(event1, ViewerEvent::ModeChanged { .. }));
        assert!(matches!(event2, ViewerEvent::ModeChanged { .. }));
    }
}
