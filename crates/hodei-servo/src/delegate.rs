use hodei_core::types::{ViewId, MetadataEvent};
use std::sync::mpsc::Sender;

/// Engine-level delegate.
pub struct HodeiServoDelegate;

impl servo::ServoDelegate for HodeiServoDelegate {
    fn notify_error(&self, error: servo::ServoError) {
        log::error!("Servo error: {:?}", error);
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
}
