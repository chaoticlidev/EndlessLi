//! Regenerates `src/ipc/bindings.ts` from the command surface.
//!
//! This lives in `tests/` rather than as a lib unit test because exporting
//! pulls in the Tauri runtime GUI code, whose test binary needs the
//! comctl32 v6 manifest that `build.rs` embeds for test targets.

use specta_typescript::Typescript;

#[test]
fn export_bindings() {
    let builder = endlessli_lib::specta_builder();
    builder
        .export(Typescript::default(), "../src/ipc/bindings.ts")
        .expect("failed to export TypeScript bindings");
    assert!(
        std::path::Path::new("../src/ipc/bindings.ts").exists(),
        "bindings file was not written"
    );
}
