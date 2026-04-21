use hodei_core::types::{ViewId, MetadataEvent, TileLoadStatus};
use std::sync::mpsc::Sender;

/// Engine-level delegate.
pub struct HodeiServoDelegate {
    pub metadata_tx: Sender<MetadataEvent>,
}

impl HodeiServoDelegate {
    pub fn new(metadata_tx: Sender<MetadataEvent>) -> Self {
        Self { metadata_tx }
    }
}

impl servo::ServoDelegate for HodeiServoDelegate {
    fn notify_error(&self, error: servo::ServoError) {
        log::error!("Servo error: {:?}", error);
    }

    fn notify_devtools_server_started(&self, port: u16, token: String) {
        log::info!("DevTools server started on port {} (token: {})", port, token);
        let _ = self.metadata_tx.send(MetadataEvent::DevToolsStarted { port, token });
    }

    fn request_devtools_connection(&self, request: servo::AllowOrDenyRequest) {
        log::info!("Auto-allowing DevTools connection request");
        request.allow();
    }
}

/// Per-WebView delegate. Routes events back to the app via channel.
pub struct HodeiWebViewDelegate {
    pub view_id: ViewId,
    pub metadata_tx: Sender<MetadataEvent>,
}

impl HodeiWebViewDelegate {
    pub fn new(view_id: ViewId, metadata_tx: Sender<MetadataEvent>) -> Self {
        Self { view_id, metadata_tx }
    }
}

impl servo::WebViewDelegate for HodeiWebViewDelegate {
    fn notify_new_frame_ready(&self, _webview: servo::WebView) {
        let _ = self.metadata_tx.send(MetadataEvent::FrameReady { view_id: self.view_id });
    }

    fn notify_url_changed(&self, _webview: servo::WebView, url: url::Url) {
        log::debug!("URL changed for {:?}: {}", self.view_id, url);
        let _ = self.metadata_tx.send(MetadataEvent::UrlChanged {
            view_id: self.view_id,
            url: url.to_string(),
        });
    }

    fn notify_page_title_changed(&self, _webview: servo::WebView, title: Option<String>) {
        log::debug!("Title changed for {:?}: {:?}", self.view_id, title);
        if let Some(title) = title {
            let _ = self.metadata_tx.send(MetadataEvent::TitleChanged {
                view_id: self.view_id,
                title,
            });
        }
    }

    fn notify_status_text_changed(&self, _webview: servo::WebView, status: Option<String>) {
        let _ = self.metadata_tx.send(MetadataEvent::StatusTextChanged {
            view_id: self.view_id,
            text: status,
        });
    }

    fn notify_load_status_changed(&self, _webview: servo::WebView, status: servo::LoadStatus) {
        let mapped = match status {
            servo::LoadStatus::Started => TileLoadStatus::Started,
            servo::LoadStatus::HeadParsed => TileLoadStatus::HeadParsed,
            servo::LoadStatus::Complete => TileLoadStatus::Complete,
        };
        let _ = self.metadata_tx.send(MetadataEvent::LoadStatusChanged {
            view_id: self.view_id,
            status: mapped,
        });
    }

    fn notify_history_changed(
        &self,
        _webview: servo::WebView,
        entries: Vec<url::Url>,
        current: usize,
    ) {
        let can_back = current > 0;
        let can_forward = current + 1 < entries.len();
        let _ = self.metadata_tx.send(MetadataEvent::HistoryChanged {
            view_id: self.view_id,
            can_back,
            can_forward,
        });
    }
}
