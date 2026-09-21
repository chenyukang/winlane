use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_FILE_PATH: &str = "~/Library/Logs/Winlane/winlane.log";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
}

impl Level {
    pub fn allows(self, event: Self) -> bool {
        event != Self::Off && event <= self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub level: Level,
    pub file_path: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            level: Level::Info,
            file_path: DEFAULT_FILE_PATH.into(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        let path = Path::new(&self.file_path);
        if self.file_path.contains(['\0', '\n', '\r'])
            || !(path.is_absolute() || self.file_path.starts_with("~/"))
            || self.file_path.ends_with('/')
            || self.file_path.ends_with("/.")
            || path.file_name().is_none()
        {
            return Err(crate::tr!(
                "日志路径须为完整文件路径，以 / 或 ~/ 开头，例如 ~/Library/Logs/Winlane/winlane.log。",
                "Enter a log file path starting with / or ~/, such as ~/Library/Logs/Winlane/winlane.log."
            ).into());
        }
        Ok(())
    }

    pub fn resolve_path(&self, home: Option<&Path>) -> Result<PathBuf, String> {
        self.validate()?;
        if let Some(relative) = self.file_path.strip_prefix("~/") {
            home.map(|home| home.join(relative)).ok_or_else(|| {
                crate::tr!("无法确定用户主目录。", "The home directory is unavailable.").into()
            })
        } else {
            Ok(PathBuf::from(&self.file_path))
        }
    }
}
