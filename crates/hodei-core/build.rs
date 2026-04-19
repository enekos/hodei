fn main() {
    println!("cargo:rerun-if-changed=../../ui/hud.slint");
    slint_build::compile("../../ui/hud.slint").unwrap();
}
