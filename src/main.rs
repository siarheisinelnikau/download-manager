#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod folder_picker;
mod icons;
mod instance;
mod theme;
mod tray;
mod ui;
mod window;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|a| a == "--probe-folder-picker") {
        if args.len() != 4 || std::env::var_os("DOWNLOAD_MANAGER_DATA_DIR").is_none() {
            return Err("Usage: --probe-folder-picker FIXTURE_FOLDER OUTPUT.json (requires isolated DOWNLOAD_MANAGER_DATA_DIR)".into());
        }
        let result = folder_picker::probe(std::path::Path::new(&args[2]));
        std::fs::write(
            &args[3],
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": download_manager::model::VERSION,
                "dialog_shown": false,
                "configured": result.is_ok(),
                "error": result.as_ref().err().map(ToString::to_string),
            }))?,
        )?;
        result?;
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--apply-update") {
        if args.len() != 6 {
            return Err("Invalid updater arguments".into());
        }
        let result = download_manager::updater::apply_update(
            std::path::Path::new(&args[2]),
            std::path::Path::new(&args[3]),
            args[4].to_str().ok_or("Invalid update checksum")?,
            args[5].to_str().ok_or("Invalid parent PID")?.parse()?,
        );
        if let Err(error) = result {
            // Helpers have no interactive UI. Keep a durable, user-readable failure report.
            let log = std::path::Path::new(&args[3]).with_file_name("last-update-error.txt");
            let _ = std::fs::write(
                log,
                format!(
                    "Update installation failed: {error:#}\nThe previous executable or its .update-backup.exe copy is preserved.\n"
                ),
            );
            return Err(error.into());
        }
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--capture-ui") {
        if !matches!(args.len(), 4 | 6) {
            return Err(
                "Usage: --capture-ui main|settings|popup|new OUTPUT.png [WIDTH HEIGHT] (requires DOWNLOAD_MANAGER_DATA_DIR)"
                    .into(),
            );
        }
        if std::env::var_os("DOWNLOAD_MANAGER_DATA_DIR").is_none() {
            return Err(
                "UI capture requires an explicit isolated DOWNLOAD_MANAGER_DATA_DIR".into(),
            );
        }
        let view = args[2].to_str().ok_or("Invalid capture view")?;
        if !matches!(view, "main" | "settings" | "popup" | "new") {
            return Err("Capture view must be main, settings, popup or new".into());
        }
        let size = if args.len() == 6 {
            let width: u32 = args[4].to_str().ok_or("Invalid width")?.parse()?;
            let height: u32 = args[5].to_str().ok_or("Invalid height")?.parse()?;
            let (min_width, min_height) = match view {
                "settings" => (560, 440),
                "popup" => (360, 220),
                _ => (760, 540),
            };
            if !(min_width..=1920).contains(&width) || !(min_height..=1080).contains(&height) {
                return Err("Capture size is outside the supported logical window bounds".into());
            }
            Some((width, height))
        } else {
            None
        };
        return window::run_capture(view, std::path::PathBuf::from(&args[3]), size);
    }
    if args.get(1).is_some_and(|a| a == "--diagnose-ui") {
        if args.len() != 4 || std::env::var_os("DOWNLOAD_MANAGER_DATA_DIR").is_none() {
            return Err("Usage: --diagnose-ui main|settings|popup|new|navigation|tray OUTPUT.json (requires DOWNLOAD_MANAGER_DATA_DIR)".into());
        }
        let view = args[2].to_str().ok_or("Invalid diagnostic view")?;
        if !matches!(
            view,
            "main" | "settings" | "popup" | "new" | "navigation" | "tray"
        ) {
            return Err(
                "Diagnostic view must be main, settings, popup, new, navigation or tray".into(),
            );
        }
        return window::run_diagnostic(view, std::path::PathBuf::from(&args[3]));
    }
    if args.get(1).is_some_and(|a| a == "--activate-only") {
        let _ = instance::acquire(
            &download_manager::storage::data_directory()?,
            std::sync::Arc::new(|| {}),
            true,
        )?;
        return Ok(());
    }
    if args.get(1).is_some_and(|a| a == "--export-icon") && args.len() == 3 {
        let path = std::path::PathBuf::from(&args[2]);
        if path.extension().is_some_and(|e| e == "ico") {
            std::fs::write(path, icons::ico())?;
        } else {
            let mut encoder = png::Encoder::new(std::fs::File::create(path)?, 64, 64);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header()?.write_image_data(&icons::rgba(64))?;
        }
        return Ok(());
    }
    if args.len() > 1 {
        return Err("Unknown command-line option".into());
    }
    window::run()
}
