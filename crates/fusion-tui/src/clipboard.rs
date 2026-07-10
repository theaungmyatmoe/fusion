pub fn save_clipboard_image(_cwd: &str) -> Result<std::path::PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        use std::time::SystemTime;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("image_{}.png", timestamp);

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let cache_dir = std::path::Path::new(&home).join(".fusioncode");
        if let Err(e) = std::fs::create_dir_all(&cache_dir) {
            return Err(format!("Failed to create cache directory: {}", e));
        }
        let target_path = cache_dir.join(&filename);
        let target_path_str = target_path.to_string_lossy();

        // Safe AppleScript command supporting both file URLs (class furl) and screenshots (class PNGf)
        let script = format!(
            "try\n\
             set file_path to POSIX path of (the clipboard as «class furl»)\n\
             return \"FILE:\" & file_path\n\
             on error\n\
             try\n\
             set png_data to the clipboard as «class PNGf»\n\
             set the_file to open for access POSIX file \"{}\" with write permission\n\
             set eof of the_file to 0\n\
             write png_data to the_file\n\
             close access the_file\n\
             return \"DATA\"\n\
             on error err\n\
             try\n\
             close access POSIX file \"{}\"\n\
             end try\n\
             error err\n\
             end try\n\
             end try",
            target_path_str, target_path_str
        );

        let output = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    let result = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if result.starts_with("FILE:") {
                        let path_str = result.strip_prefix("FILE:").unwrap_or(&result);
                        let path = std::path::PathBuf::from(path_str);
                        if path.exists() && path.is_file() {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                                let ext_lower = ext.to_lowercase();
                                if ext_lower == "png"
                                    || ext_lower == "jpg"
                                    || ext_lower == "jpeg"
                                    || ext_lower == "webp"
                                    || ext_lower == "gif"
                                {
                                    return Ok(path);
                                }
                            }
                        }
                        Err("Clipboard contains a file, but it is not a supported image format.".to_string())
                    } else {
                        Ok(target_path)
                    }
                } else {
                    let err_msg = String::from_utf8_lossy(&out.stderr);
                    if err_msg.contains("-1700") || err_msg.contains("-2700") {
                        Err("No image or image file found in clipboard.".to_string())
                    } else {
                        Err(format!("AppleScript error: {}", err_msg.trim()))
                    }
                }
            }
            Err(e) => Err(format!("Failed to execute osascript: {}", e)),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _cwd = cwd;
        Err("Clipboard image extraction is only supported on macOS.".to_string())
    }
}

pub fn get_clipboard_text() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("pbpaste")
            .output();

        match output {
            Ok(out) => {
                if out.status.success() {
                    Ok(String::from_utf8_lossy(&out.stdout).to_string())
                } else {
                    let err_msg = String::from_utf8_lossy(&out.stderr);
                    Err(format!("pbpaste error: {}", err_msg.trim()))
                }
            }
            Err(e) => Err(format!("Failed to execute pbpaste: {}", e)),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Clipboard text extraction is only supported on macOS.".to_string())
    }
}

pub fn edit_text_in_editor(seed: &str) -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        use std::env;
        use std::fs;
        use std::process::Command;

        let editor = env::var("VISUAL")
            .or_else(|_| env::var("EDITOR"))
            .unwrap_or_else(|_| "nano".to_string());

        let temp_dir = env::temp_dir();
        let temp_file = temp_dir.join("fusion_message.md");
        if let Err(e) = fs::write(&temp_file, seed) {
            return Err(format!("Failed to write temp file: {}", e));
        }

        let mut cmd = Command::new(&editor);
        cmd.arg(&temp_file)
            .stdin(std::process::Stdio::inherit())
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit());

        let status = cmd.status();

        match status {
            Ok(s) => {
                if s.success() {
                    fs::read_to_string(&temp_file)
                        .map_err(|e| format!("Failed to read temp file: {}", e))
                } else {
                    Err(format!("Editor {} exited with non-zero status", editor))
                }
            }
            Err(e) => Err(format!("Failed to launch editor {}: {}", editor, e)),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _seed = seed;
        Err("External editor is only supported on macOS.".to_string())
    }
}
