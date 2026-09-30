use std::path::PathBuf;
#[cfg(windows)]
use std::process::Command;

pub struct Startup {
    marker: PathBuf,
}

impl Startup {
    pub fn new() -> Self {
        let base = directories::BaseDirs::new();
        let marker = base
            .map(|b| b.config_dir().join("adhd").join("installed"))
            .unwrap_or_default();
        Self { marker }
    }
    pub fn available(&self) -> bool {
        self.marker.is_file()
    }

    pub fn enabled(&self) -> bool {
        if !self.available() {
            return false;
        }
        #[cfg(target_os = "linux")]
        {
            directories::BaseDirs::new().is_some_and(|b| {
                b.config_dir()
                    .join("autostart/io.github.adhd.Reminder.desktop")
                    .is_file()
            })
        }
        #[cfg(windows)]
        {
            !self.marker.with_file_name("startup-disabled").exists()
        }
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        if !self.available() {
            return Err("Install ADHD first to manage login startup.".into());
        }
        #[cfg(target_os = "linux")]
        {
            let base =
                directories::BaseDirs::new().ok_or("Cannot find your configuration folder")?;
            let path = base
                .config_dir()
                .join("autostart/io.github.adhd.Reminder.desktop");
            if enabled {
                let template = self.marker.with_file_name("autostart.desktop");
                std::fs::create_dir_all(path.parent().ok_or("Invalid autostart path")?)
                    .map_err(|e| e.to_string())?;
                std::fs::copy(template, path).map_err(|e| e.to_string())?;
            } else if path.exists() {
                std::fs::remove_file(path).map_err(|e| e.to_string())?;
            }
            Ok(())
        }
        #[cfg(windows)]
        {
            let action = if enabled { "/ENABLE" } else { "/DISABLE" };
            let task = std::fs::read_to_string(self.marker.with_file_name("task-name"))
                .map_err(|_| "Run the Windows installer again to register startup.".to_string())?;
            let mut cmd = Command::new("schtasks.exe");
            use std::os::windows::process::CommandExt;
            let status = cmd
                .creation_flags(0x08000000)
                .args(["/Change", "/TN", task.trim(), action])
                .status()
                .map_err(|e| e.to_string())?;
            if !status.success() {
                return Err("Could not update login startup. Run the installer again.".into());
            }
            let marker = self.marker.with_file_name("startup-disabled");
            if enabled {
                if marker.exists() {
                    std::fs::remove_file(marker).map_err(|e| e.to_string())?;
                }
            } else {
                std::fs::write(marker, "").map_err(|e| e.to_string())?;
            }
            Ok(())
        }
    }
}
