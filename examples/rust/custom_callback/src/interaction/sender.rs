use std::sync::Arc;
use std::time::Duration;

use rerun::external::re_log;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::Mutex;

use super::protocol::ViewerEvent;

/// Sends ViewerEvents to a Python bridge over TCP.
/// 
/// This sender connects to the Python ViewerBridge (which acts as a server)
/// and sends length-prefixed bincode messages.
#[derive(Debug)]
pub struct ViewerEventSender {
    address: String,
    tx: UnboundedSender<ViewerEvent>,
    rx: Arc<Mutex<UnboundedReceiver<ViewerEvent>>>,
}

/// A cloneable handle for sending events.
#[derive(Clone)]
pub struct ViewerEventSenderHandle {
    tx: UnboundedSender<ViewerEvent>,
}

impl ViewerEventSenderHandle {
    /// Send a ViewerEvent (non-blocking, queued).
    pub fn send(&self, event: ViewerEvent) -> Result<(), tokio::sync::mpsc::error::SendError<ViewerEvent>> {
        self.tx.send(event)
    }
}

impl ViewerEventSender {
    /// Create a new sender (not yet connected).
    pub fn new(address: String) -> Self {
        #[expect(clippy::disallowed_methods)]
        let (tx, rx) = unbounded_channel();
        Self {
            address,
            tx,
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    /// Get a cloneable handle for sending events.
    pub fn handle(&self) -> ViewerEventSenderHandle {
        ViewerEventSenderHandle {
            tx: self.tx.clone(),
        }
    }

    /// Run the sender (connect and send events in a loop).
    /// This should be spawned in a tokio task.
    pub async fn run(self) {
        re_log::info!("ViewerEventSender: Starting");

        loop {
            match TcpStream::connect(&self.address).await {
                Ok(mut socket) => {
                    re_log::info!("ViewerEventSender: Connected to {}", self.address);
                    
                    // Send events until connection fails
                    let mut rx = self.rx.lock().await;
                    while let Some(event) = rx.recv().await {
                        match Self::send_event(&mut socket, &event).await {
                            Ok(()) => {
                                re_log::debug!("ViewerEventSender: Sent event: {:?}", event);
                            }
                            Err(err) => {
                                re_log::error!("ViewerEventSender: Failed to send event: {:?}", err);
                                break; // Connection lost, reconnect
                            }
                        }
                    }
                    drop(rx); // Release lock before reconnecting
                }
                Err(err) => {
                    re_log::error!("ViewerEventSender: Failed to connect to {}: {:?}", self.address, err);
                }
            }

            // Wait before reconnecting
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    /// Send a single event (length-prefixed bincode).
    async fn send_event(socket: &mut TcpStream, event: &ViewerEvent) -> tokio::io::Result<()> {
        let encoded = event.encode()?;
        let len = encoded.len() as u32;
        
        // Send length prefix (4 bytes, big-endian)
        socket.write_all(&len.to_be_bytes()).await?;
        
        // Send payload
        socket.write_all(&encoded).await?;
        
        Ok(())
    }
}
