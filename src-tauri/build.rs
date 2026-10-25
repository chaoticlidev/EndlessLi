fn main() {
    // Write the .rc and .manifest files if they don't exist
    #[cfg(windows)]
    {
        let manifest = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
    </dependentAssembly>
  </dependency>
</assembly>"#;

        std::fs::write("windows-app.manifest", manifest).unwrap();
        std::fs::write("windows-app.rc", "1 24 \"windows-app.manifest\"").unwrap();
    }

    #[allow(unused_mut)]
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
