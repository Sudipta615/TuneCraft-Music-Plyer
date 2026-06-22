// slint-build.rs — compile .slint markup → Rust at build time.

fn main() {
    // Use an include path so .slint files in `components/` can import
    // `theme/colors.slint`, `types.slint`, etc. without `../` prefixes.
    let config = slint_build::CompilerConfiguration::new()
        .with_include_paths(vec!["ui".into()]);

    slint_build::compile_with_config("ui/app.slint", config)
        .expect("Slint UI compilation failed");

    println!("cargo:rerun-if-changed=ui/app.slint");
    println!("cargo:rerun-if-changed=ui/types.slint");
    println!("cargo:rerun-if-changed=ui/theme/colors.slint");
    println!("cargo:rerun-if-changed=ui/theme/widgets.slint");
    println!("cargo:rerun-if-changed=ui/components/sidebar.slint");
    println!("cargo:rerun-if-changed=ui/components/player_bar.slint");
    println!("cargo:rerun-if-changed=ui/components/track_list.slint");
    println!("cargo:rerun-if-changed=ui/components/eq_panel.slint");
    println!("cargo:rerun-if-changed=ui/components/folders_view.slint");
    println!("cargo:rerun-if-changed=ui/components/settings_view.slint");
    println!("cargo:rerun-if-changed=ui/components/toasts.slint");
    println!("cargo:rerun-if-changed=ui/components/dialogs.slint");
    println!("cargo:rerun-if-changed=ui/components/lyrics_panel.slint");
}
