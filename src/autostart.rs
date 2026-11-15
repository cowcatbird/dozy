//! Launch at login, without a helper app: a per-user LaunchAgent.

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
