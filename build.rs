use std::env;
fn main() {
    println!("cargo:rustc-check-cfg=cfg(bundled_tools)");
    println!("cargo:rustc-check-cfg=cfg(bundled_installer)");
    let stub = env::var_os("CARGO_FEATURE_SETUP_STUB").is_some();
    let builder = env::var_os("CARGO_FEATURE_SETUP_BUILDER").is_some();
    println!("cargo:rerun-if-changed=resources/tools.zip");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::path::Path::new("resources/tools.zip").is_file()
        && !stub
        && !builder
    {
        println!("cargo:rustc-cfg=bundled_tools");
    }
    println!("cargo:rerun-if-changed=resources/setup-stub.exe");
    if !stub
        && env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::path::Path::new("resources/setup-stub.exe").is_file()
    {
        println!("cargo:rustc-cfg=bundled_installer");
    }
    println!("cargo:rerun-if-changed=assets/workflow.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resource = winresource::WindowsResource::new();
        resource
            .set_icon("assets/workflow.ico")
            .set("ProductName", "WorkFlow Media")
            .set(
                "FileDescription",
                if env::var_os("CARGO_FEATURE_UPDATE_HELPER").is_some() {
                    "WorkFlow Media · Update helper"
                } else if stub {
                    "WorkFlow Media · Web installer"
                } else if builder {
                    "WorkFlow Media · Setup Builder"
                } else {
                    "WorkFlow Media · Rust"
                },
            );
        resource.set_manifest(r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0"><dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency><trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"/></requestedPrivileges></security></trustInfo><application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings><dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness><longPathAware xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">true</longPathAware></windowsSettings></application></assembly>"#);
        resource.compile().expect("Windows resource compiler");
    }
}
