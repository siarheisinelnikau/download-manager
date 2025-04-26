#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod ui;
mod window;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().collect();
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
    if args.len() > 1 {
        return Err("Unknown command-line option".into());
    }
    window::run()
}
