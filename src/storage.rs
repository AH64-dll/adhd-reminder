use crate::core::State;
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> io::Result<Self> {
        fs::create_dir_all(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { dir })
    }

    pub fn load(&self) -> (State, Option<String>) {
        let primary = self.dir.join("state.json");
        match read_state(&primary) {
            Ok(s) => (s, None),
            Err(err) => {
                if let Ok(s) = read_state(&self.dir.join("state.backup.json")) {
                    (
                        s,
                        Some("Recovered your goal from the last saved backup.".into()),
                    )
                } else if err.kind() == io::ErrorKind::NotFound {
                    (State::default(), None)
                } else {
                    (State::default(), Some("Saved data could not be read. A new session is ready; the old files have been preserved.".into()))
                }
            }
        }
    }

    pub fn save(&self, state: &State) -> io::Result<()> {
        state.validate().map_err(io::Error::other)?;
        // Preserve only a known-valid predecessor, never rotate corruption into the backup.
        let primary = self.dir.join("state.json");
        if let Ok(old) = read_state(&primary) {
            atomic_write(
                &self.dir.join("state.backup.json"),
                &serde_json::to_vec(&old)?,
            )?;
        } else if primary.exists() {
            fs::copy(&primary, self.dir.join("state.corrupt.json"))?;
        }
        atomic_write(&primary, &serde_json::to_vec_pretty(state)?)
    }

    pub fn log(&self, message: &str) {
        let path = self.dir.join("diagnostic.log");
        if fs::metadata(&path).is_ok_and(|m| m.len() > 256 * 1024) {
            let _ = fs::remove_file(self.dir.join("diagnostic.previous.log"));
            let _ = fs::rename(&path, self.dir.join("diagnostic.previous.log"));
        }
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{} {message}", crate::ipc::now());
        }
    }
}

fn read_state(path: &Path) -> io::Result<State> {
    if fs::metadata(path)?.len() > 64 * 1024 {
        return Err(io::Error::other("State file is too large"));
    }
    let state: State = serde_json::from_slice(&fs::read(path)?)?;
    state.validate().map_err(io::Error::other)?;
    Ok(state)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing parent directory"))?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_round_trip_and_backup_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_owned()).unwrap();
        let mut s = State::default();
        s.start("First", 0, 20).unwrap();
        store.save(&s).unwrap();
        s.start("Second", 0, 20).unwrap();
        store.save(&s).unwrap();
        assert_eq!(store.load().0.goal.as_deref(), Some("Second"));
        fs::write(dir.path().join("state.json"), b"broken").unwrap();
        let (recovered, warning) = store.load();
        assert_eq!(recovered.goal.as_deref(), Some("First"));
        assert!(warning.is_some());
        store.save(&recovered).unwrap();
        assert_eq!(
            fs::read(dir.path().join("state.corrupt.json")).unwrap(),
            b"broken"
        );
    }
    #[test]
    fn unwritable_store_reports_failure() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().to_owned()).unwrap();
        fs::create_dir(dir.path().join("state.json")).unwrap();
        assert!(store.save(&State::default()).is_err());
    }
}
