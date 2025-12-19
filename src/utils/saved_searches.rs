use std::fs;
use std::path::PathBuf;
use std::io;
#[cfg(not(test))]
use directories::ProjectDirs;

pub struct SavedSearchManager;

impl SavedSearchManager {
    #[cfg(not(test))]
    fn get_storage_dir() -> PathBuf {
        // Try ~/.config/splunk-tui/saved_searches using directories crate
        if let Some(proj_dirs) = ProjectDirs::from("", "", "splunk-tui") {
             let mut path = proj_dirs.config_dir().to_path_buf();
             path.push("saved_searches");
             if fs::create_dir_all(&path).is_ok() {
                 return path;
             }
        }

        // Fallback to local directory
        let path = PathBuf::from("saved_searches");
        let _ = fs::create_dir_all(&path);
        path
    }

    #[cfg(test)]
    fn get_storage_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push("splunk-tui-tests");
        path.push("saved_searches");
        let _ = fs::create_dir_all(&path);
        path
    }

    pub fn list_searches() -> io::Result<Vec<String>> {
        let dir = Self::get_storage_dir();
        let mut searches = Vec::new();

        if dir.exists() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("spl") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        searches.push(name.to_string());
                    }
                }
            }
        }
        searches.sort();
        Ok(searches)
    }

    pub fn save_search(name: &str, query: &str) -> io::Result<()> {
        let mut path = Self::get_storage_dir();
        path.push(format!("{}.spl", name));
        fs::write(path, query)
    }

    pub fn load_search(name: &str) -> io::Result<String> {
        let mut path = Self::get_storage_dir();
        path.push(format!("{}.spl", name));
        fs::read_to_string(path)
    }
}
