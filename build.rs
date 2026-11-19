fn main() {
    println!("cargo:rerun-if-changed=macos/settings.m");
    println!("cargo:rerun-if-changed=macos/preferences.h");
    println!("cargo:rerun-if-changed=macos/preferences.m");
    cc::Build::new()
        .file("macos/settings.m")
        .file("macos/preferences.m")
        .flag("-fobjc-arc")
        .flag("-fblocks")
        .compile("minterm_settings");
    if std::env::var_os("CARGO_FEATURE_UI_TESTS").is_some() {
        println!("cargo:rerun-if-changed=tests/macos/driver.m");
        println!("cargo:rerun-if-changed=tests/macos/preferences_driver.m");
        cc::Build::new()
            .file("tests/macos/driver.m")
            .flag("-fobjc-arc")
            .flag("-fblocks")
            .compile("minterm_ui_test");
    }
    println!("cargo:rustc-link-lib=framework=AppKit");
    println!("cargo:rustc-link-lib=framework=Foundation");
}
