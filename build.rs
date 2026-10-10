fn main() {
    // Windows gives the main thread only 1 MB of stack, and GPUI renders the
    // whole UI on it; debug builds keep every builder temporary on the stack,
    // so deep views can overflow it. 8 MB matches what Zed (same UI stack)
    // sets for its Windows binary.
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if os == "windows" {
        if env == "msvc" {
            println!("cargo:rustc-link-arg-bins=/STACK:8388608");
            embed_windows_resources();
        } else {
            println!("cargo:rustc-link-arg-bins=-Wl,--stack,8388608");
        }
    }
}

/// Compiles the app icon and version info into the .exe with the Windows
/// SDK's `rc.exe` (no extra crate). Icon ID 1 is what Explorer shows for the
/// file and what GPUI loads for the window title bar and taskbar. If `rc.exe`
/// can't be found the build still succeeds, just without the icon.
fn embed_windows_resources() {
    use std::path::PathBuf;
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let icon = manifest.join("assets").join("icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe (Windows SDK) not found; the .exe is built without its icon");
        return;
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap_or_default());
    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into());
    let mut numbers: Vec<u32> = version.split(['.', '-', '+']).take(3).map(|part| part.parse().unwrap_or(0)).collect();
    numbers.resize(3, 0);
    let (major, minor, patch) = (numbers[0], numbers[1], numbers[2]);
    let script = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Awan Host Remote"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "awan-host-remote"
      VALUE "OriginalFilename", "awan-host-remote.exe"
      VALUE "ProductName", "Awan Host Remote"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        // rc.exe string literals treat backslashes as escapes.
        icon = icon.display().to_string().replace('\\', "\\\\"),
    );
    let rc_file = out.join("app.rc");
    let res_file = out.join("app.res");
    if let Err(error) = std::fs::write(&rc_file, script) {
        println!("cargo:warning=cannot write {}: {error}", rc_file.display());
        return;
    }
    let status = std::process::Command::new(&rc).arg("/nologo").arg("/fo").arg(&res_file).arg(&rc_file).status();
    match status {
        // The MSVC linker takes a compiled .res file directly.
        Ok(status) if status.success() => println!("cargo:rustc-link-arg-bins={}", res_file.display()),
        Ok(status) => println!("cargo:warning=rc.exe failed ({status}); the .exe is built without its icon"),
        Err(error) => println!("cargo:warning=cannot run rc.exe: {error}; the .exe is built without its icon"),
    }
}

/// `rc.exe` from PATH (a Developer Prompt) or the newest Windows 10/11 SDK.
fn find_rc() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("rc.exe");
            if candidate.is_file() { return Some(candidate); }
        }
    }
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => "x64",
    };
    let roots = ["ProgramFiles(x86)", "ProgramFiles"].into_iter()
        .filter_map(std::env::var_os)
        .map(|root| PathBuf::from(root).join("Windows Kits").join("10").join("bin"));
    let mut found: Vec<PathBuf> = roots
        .filter_map(|bin| std::fs::read_dir(bin).ok())
        .flat_map(|entries| entries.filter_map(Result::ok).map(|entry| entry.path()))
        .map(|version| version.join(arch).join("rc.exe"))
        .filter(|rc| rc.is_file())
        .collect();
    // Version folders are named 10.0.<build>.0; the highest sorts last.
    found.sort_by_key(|rc| {
        rc.parent().and_then(|dir| dir.parent()).and_then(|dir| dir.file_name())
            .map(|name| name.to_string_lossy().split('.').map(|part| part.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>())
            .unwrap_or_default()
    });
    found.pop()
}
