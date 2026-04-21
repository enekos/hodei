//! Stub WebSocket bridge for Servo DevTools.
//!
//! The real bridge is planned to proxy Servo's DevTools TCP server over a
//! WebSocket that external inspectors can speak to. Until that lands, this
//! stub exists so the rest of the app compiles and logs the configured URL.

pub struct DevToolsBridge {
    tcp_port: u16,
    ws_port: u16,
    running: bool,
}

impl DevToolsBridge {
    pub fn new(tcp_port: u16, ws_port: u16) -> Self {
        Self { tcp_port, ws_port, running: false }
    }

    pub fn spawn(&mut self) {
        self.running = true;
        log::info!(
            "DevTools bridge (stub): would proxy ws://127.0.0.1:{} -> tcp://127.0.0.1:{}",
            self.ws_port, self.tcp_port
        );
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/", self.ws_port)
    }
}
