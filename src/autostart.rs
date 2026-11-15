//! Launch at login, without a helper app: a LaunchAgent on macOS, the per-user Run key on Windows.

#[cfg(target_os = "macos")]
pub fn set(enabled: bool) -> std::io::Result<()> {
    let Some(home) = dirs::home_dir() else { return Ok(()) };
    let plist = home.join("Library/LaunchAgents/app.dozy.plist");
    if !enabled {
        return match std::fs::remove_file(&plist) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    let exe = std::env::current_exe()?;
    let exe = exe.to_string_lossy().replace('&', "&amp;").replace('<', "&lt;");
    std::fs::create_dir_all(plist.parent().unwrap())?;
    std::fs::write(
        plist,
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key><string>app.dozy</string>
    <key>ProgramArguments</key><array><string>{exe}</string></array>
    <key>RunAtLoad</key><true/>
    <key>ProcessType</key><string>Interactive</string>
</dict>
</plist>
"#
        ),
    )
}

#[cfg(windows)]
pub fn set(enabled: bool) -> std::io::Result<()> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, REG_SZ, RegDeleteKeyValueW, RegSetKeyValueW};

    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let key = wide(r"Software\Microsoft\Windows\CurrentVersion\Run");
    let name = wide("Dozy");
    let status = if enabled {
        let command = wide(&format!("\"{}\"", std::env::current_exe()?.display()));
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                name.as_ptr(),
                REG_SZ,
                command.as_ptr().cast(),
                (command.len() * 2) as u32,
            )
        }
    } else {
        match unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) } {
            2 => 0, // ERROR_FILE_NOT_FOUND: already off
            status => status,
        }
    };
    match status {
        0 => Ok(()),
        code => Err(std::io::Error::from_raw_os_error(code as i32)),
    }
}
