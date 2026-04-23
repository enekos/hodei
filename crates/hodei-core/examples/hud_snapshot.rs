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
    let mut hud = Hud::new(1280, 720, 1.0);

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

    // COMMAND mode with suggestions
    hud.set_suggestions(vec![
        ("Servo — parallel browser engine".to_string(), "https://servo.org/".to_string(), true),
        ("Servo/servo GitHub".to_string(), "https://github.com/servo/servo".to_string(), false),
        ("ServoShell Examples".to_string(), "https://servo.org/shell".to_string(), false),
    ]);
    hud.set_suggestions_visible(true);
    save_png(&mut hud, "command_with_suggestions");

    // SEARCH mode
    hud.set_command_visible(false);
    hud.set_suggestions_visible(false);
    hud.set_mode_text("SEARCH");
    hud.set_search_visible(true);
    hud.set_search_text("lightweight");
    hud.set_search_info("2/7 matches");
    save_png(&mut hud, "search");

    // HINT mode with labels overlaid
    hud.set_search_visible(false);
    hud.set_mode_text("HINT");
    hud.set_hints(vec![
        ("aa".to_string(), 120.0, 140.0, false),
        ("as".to_string(), 260.0, 200.0, true),
        ("ad".to_string(), 480.0, 300.0, false),
        ("af".to_string(), 700.0, 420.0, false),
        ("ag".to_string(), 940.0, 560.0, false),
    ]);
    save_png(&mut hud, "hints");
}
