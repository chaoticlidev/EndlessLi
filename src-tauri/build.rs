fn main() {
    tauri_build::build();
    embed_test_manifest();
}

/// Test binaries link GUI code (e.g. tao imports `TaskDialogIndirect` from
/// comctl32 v6) but, unlike the app binary, receive no application manifest
/// from `tauri_build`. Without the v6 common-controls dependency the loader
/// resolves comctl32 v5 and the test process dies with
/// `STATUS_ENTRYPOINT_NOT_FOUND` before running anything. Embedding this
/// minimal manifest for test targets only keeps `cargo test` working.
#[cfg(windows)]
fn embed_test_manifest() {
    const MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>"#;
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    std::fs::write(out.join("test-manifest.xml"), MANIFEST).expect("write manifest");
    std::fs::write(out.join("test-manifest.rc"), "1 24 \"test-manifest.xml\"").expect("write rc");
    let _ = embed_resource::compile_for_tests(out.join("test-manifest.rc"), embed_resource::NONE);
}

/// Non-Windows targets need no manifest.
#[cfg(not(windows))]
fn embed_test_manifest() {}
