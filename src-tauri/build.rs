fn main() {
    let mut attributes = tauri_build::Attributes::new();

    #[cfg(windows)]
    {
        // Tell Tauri not to embed its own manifest
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());

        // This embeds the manifest into ALL targets (bins, tests, benches)
        embed_resource::compile_for_everything("windows-app.rc", None::<&str>)
            .manifest_required()
            .expect("failed to embed manifest");
    }

    tauri_build::try_build(attributes).expect("tauri-build failed");

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=windows-app.rc");
        println!("cargo:rerun-if-changed=windows-app.manifest");
    }
}
