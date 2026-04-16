use orthogonal_core::types::{ViewId, MetadataEvent};
use std::sync::mpsc::Sender;

/// Engine-level delegate.
pub struct OrthoServoDelegate;

impl servo::ServoDelegate for OrthoServoDelegate {
    fn notify_error(&self, error: servo::ServoError) {
        log::error!("Servo error: {:?}", error);
    }
}

/// Per-WebView delegate. Routes events back to the app via channel.
pub struct OrthoWebViewDelegate {
    pub view_id: ViewId,
    pub metadata_tx: Sender<MetadataEvent>,
}

impl OrthoWebViewDelegate {
    pub fn new(view_id: ViewId, metadata_tx: Sender<MetadataEvent>) -> Self {
        Self { view_id, metadata_tx }
    }
}

impl servo::WebViewDelegate for OrthoWebViewDelegate {
    fn notify_new_frame_ready(&self, _webview: servo::WebView) {
        log::trace!("New frame ready for {:?}", self.view_id);
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
}
