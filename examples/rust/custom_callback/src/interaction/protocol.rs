use std::io::{self, ErrorKind};
use serde::{Deserialize, Serialize};

/// Events sent from the viewer to the application.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ViewerEvent {
    /// User clicked in a spatial view.
    Click {
        position: [f32; 3],
        entity_path: Option<String>,
        view_id: String,
        timestamp_ms: u64,
        is_2d: bool,
    },
    
    /// Waypoint sequence completed.
    WaypointComplete {
        waypoints: Vec<[f32; 3]>,
    },
    
    /// Interaction mode changed.
    ModeChanged {
        mode: String,
    },
    
    /// Viewer is disconnecting.
    Disconnect,
}

/// Commands sent from the application to the viewer.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum AppCommand {
    /// Change the interaction mode.
    SetMode {
        mode: String,
    },
    
    /// Clear all waypoint markers.
    ClearWaypoints,
    
    /// Set the cursor style.
    SetCursor {
        cursor: String,
    },
}

impl ViewerEvent {
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        bincode::serialize(self).map_err(|err| io::Error::new(ErrorKind::InvalidData, err))
    }
    
    pub fn decode(data: &[u8]) -> io::Result<Self> {
        bincode::deserialize(data).map_err(|err| io::Error::new(ErrorKind::InvalidData, err))
    }
}

impl AppCommand {
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        bincode::serialize(self).map_err(|err| io::Error::new(ErrorKind::InvalidData, err))
    }
    
    pub fn decode(data: &[u8]) -> io::Result<Self> {
        bincode::deserialize(data).map_err(|err| io::Error::new(ErrorKind::InvalidData, err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_viewer_event_click_roundtrip() {
        let event = ViewerEvent::Click {
            position: [1.0, 2.0, 3.0],
            entity_path: Some("world/robot".to_string()),
            view_id: "view_123".to_string(),
            timestamp_ms: 1234567890,
            is_2d: false,
        };
        
        let encoded = event.encode().unwrap();
        let decoded = ViewerEvent::decode(&encoded).unwrap();
        
        assert_eq!(event, decoded);
    }
}
