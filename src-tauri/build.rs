fn main() {
    // Native Android commands still pass through Tauri's plugin ACL before
    // automatic Kotlin dispatch. Register only this inlined plugin's manifest;
    // Android's main-window capability grants its three used commands.
    tauri_build::try_build(
        tauri_build::Attributes::new().plugin("file-helper", tauri_build::InlinedPlugin::new()),
    )
    .unwrap_or_else(|error| panic!("failed to build Tauri application: {error}"));
}
