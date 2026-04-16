use orthogonal_core::types::ViewId;

/// Engine-level delegate. Most methods are no-ops for v0.1.0.
pub struct OrthoServoDelegate;

impl servo::ServoDelegate for OrthoServoDelegate {
    fn notify_error(&self, error: servo::ServoError) {
        log::error!("Servo error: {:?}", error);
    }
}

/// Per-WebView delegate. Routes events back to the app.
pub struct OrthoWebViewDelegate {
    pub view_id: ViewId,
}

impl OrthoWebViewDelegate {
    pub fn new(view_id: ViewId) -> Self {
        Self { view_id }
    }
}

impl servo::WebViewDelegate for OrthoWebViewDelegate {
    fn notify_new_frame_ready(&self, _webview: servo::WebView) {
        log::trace!("New frame ready for {:?}", self.view_id);
    }

    fn notify_url_changed(&self, _webview: servo::WebView, url: url::Url) {
        log::debug!("URL changed for {:?}: {}", self.view_id, url);
    }

    fn notify_page_title_changed(&self, _webview: servo::WebView, title: Option<String>) {
        log::debug!("Title changed for {:?}: {:?}", self.view_id, title);
    }
}
