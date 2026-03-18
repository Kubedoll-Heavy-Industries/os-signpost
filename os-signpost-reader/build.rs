//! Build script for os-signpost-reader.
//!
//! Sets the framework search path to include Apple's private frameworks
//! directory so that `LoggingSupport.framework` can be linked.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    // Use xcrun to find the SDK path, then fall back to the runtime path.
    // The private frameworks live under the SDK for build-time linking and
    // under /System/Library/PrivateFrameworks at runtime.
    let sdk_private_frameworks = std::process::Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                let sdk = String::from_utf8(out.stdout).ok()?;
                let sdk = sdk.trim();
                Some(format!("{sdk}/System/Library/PrivateFrameworks"))
            } else {
                None
            }
        });

    if let Some(path) = sdk_private_frameworks {
        println!("cargo:rustc-link-search=framework={path}");
    }

    // Also add the runtime path as fallback (needed when SDK path doesn't
    // contain the private framework, e.g. Command Line Tools only).
    println!("cargo:rustc-link-search=framework=/System/Library/PrivateFrameworks");

    // Explicitly request linking against LoggingSupport.
    println!("cargo:rustc-link-lib=framework=LoggingSupport");
}
