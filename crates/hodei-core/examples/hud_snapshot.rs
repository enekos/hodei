//! Render the HUD to PNGs for visual verification.
//!
//! Usage: `cargo run -p hodei-core --example hud_snapshot`
//! Output: `target/hud-snapshots/<state>.png`
use hodei_core::hud::Hud;

fn save_png(hud: &mut Hud, name: &str) {
    let w = hud.width();
    let h = hud.height();
    let buf = hud.render().to_vec();
    let out_dir = std::path::Path::new("target").join("hud-snapshots");
    std::fs::create_dir_all(&out_dir).expect("mkdir hud-snapshots");
    let path = out_dir.join(format!("{}.png", name));
    let img = image::RgbaImage::from_raw(w, h, buf).expect("raw buf");
    img.save(&path).expect("save png");
    println!("wrote {}", path.display());
}

fn main() {
    let mut hud = Hud::new(1280, 720);

    // Baseline NORMAL mode, https, bookmarked, zoomed 125%
    hud.set_mode_text("NORMAL");
    hud.set_url_text("https://servo.org/");
    hud.set_title_text("Servo, the parallel browser engine");
    hud.set_tile_count(3);
    hud.set_secure(true);
    hud.set_insecure(false);
    hud.set_bookmarked(true);
    hud.set_loading(false);
    hud.set_zoom(1.25);
    hud.set_can_back(true);
    hud.set_can_forward(false);
    save_png(&mut hud, "normal_secure_bookmarked");

    // HTTP, loading, no bookmark, zoom reset
    hud.set_url_text("http://example.invalid/page");
    hud.set_title_text("Example");
    hud.set_secure(false);
    hud.set_insecure(true);
    hud.set_bookmarked(false);
    hud.set_loading(true);
    hud.set_zoom(1.0);
    save_png(&mut hud, "normal_insecure_loading");

    // INSERT mode hides the top icon bar
    hud.set_mode_text("INSERT");
    hud.set_loading(false);
    save_png(&mut hud, "insert");

    // COMMAND mode
    hud.set_mode_text("COMMAND");
    hud.set_command_visible(true);
    hud.set_command_text("open serv");
    save_png(&mut hud, "command");
}
