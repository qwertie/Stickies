//! Plain-folder note storage. One folder per note under `<data>/notes`, each holding `note.md`
//! (YAML frontmatter + Markdown body) and an optional `attachments/` folder. Closed notes move to
//! `<data>/archive` and are purged after ARCHIVE_DAYS.

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const ARCHIVE_DAYS: i64 = 30;
pub const DEFAULT_COLOR: &str = "#FFF7B1";

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub data_dir: Option<PathBuf>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteInfo {
    pub folder: String,
    pub created: String,
    pub title: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchivedInfo {
    pub folder: String,
    pub archived: String,
    pub title: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFile {
    pub rel: String,
    pub name: String,
    pub is_dir: bool,
    pub is_image: bool,
}

pub struct Store {
    pub data_dir: PathBuf,
}

impl Store {
    pub fn open() -> Store {
        let data_dir = load_config().data_dir.unwrap_or_else(default_data_dir);
        let store = Store { data_dir };
        fs::create_dir_all(store.notes_dir()).ok();
        fs::create_dir_all(store.archive_dir()).ok();
        store
    }

    pub fn notes_dir(&self) -> PathBuf {
        self.data_dir.join("notes")
    }
    pub fn archive_dir(&self) -> PathBuf {
        self.data_dir.join("archive")
    }
    pub fn note_dir(&self, folder: &str) -> PathBuf {
        self.notes_dir().join(folder)
    }
    pub fn note_file(&self, folder: &str) -> PathBuf {
        self.note_dir(folder).join("note.md")
    }

    pub fn list_notes(&self) -> Vec<NoteInfo> {
        let mut notes: Vec<NoteInfo> = list_subfolders(&self.notes_dir())
            .into_iter()
            .filter_map(|folder| {
                let content = fs::read_to_string(self.note_file(&folder)).ok()?;
                let created = get_frontmatter(&content, "created")
                    .unwrap_or_else(|| file_time(&self.note_file(&folder)));
                Some(NoteInfo { title: title_of(&content), folder, created })
            })
            .collect();
        notes.sort_by(|a, b| a.created.cmp(&b.created));
        notes
    }

    pub fn read_note(&self, folder: &str) -> Result<String, String> {
        fs::read_to_string(self.note_file(folder)).map_err(|e| e.to_string())
    }

    /// Writes `note.md` atomically (temp file + rename) so the watcher never sees a torn file.
    pub fn write_note(&self, folder: &str, content: &str) -> Result<(), String> {
        let dir = self.note_dir(folder);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let tmp = dir.join("note.md.tmp");
        fs::write(&tmp, content).map_err(|e| e.to_string())?;
        fs::rename(&tmp, self.note_file(folder)).map_err(|e| e.to_string())
    }

    pub fn create_note(&self) -> Result<String, String> {
        let now = Local::now();
        let base = now.format("%Y-%m-%d-%H%M%S").to_string();
        let mut folder = base.clone();
        let mut n = 1;
        while self.note_dir(&folder).exists() {
            n += 1;
            folder = format!("{base}-{n}");
        }
        let content = format!("---\ncolor: \"{}\"\ncreated: {}\n---\n\n", DEFAULT_COLOR, now_iso());
        self.write_note(&folder, &content)?;
        Ok(folder)
    }

    pub fn archive_note(&self, folder: &str) -> Result<(), String> {
        let src = self.note_dir(folder);
        let content = fs::read_to_string(src.join("note.md")).unwrap_or_default();
        let content = set_frontmatter(&content, "archived", &now_iso());
        fs::write(src.join("note.md"), content).map_err(|e| e.to_string())?;
        let dest = unique_dest(&self.archive_dir(), folder);
        fs::rename(&src, dest).map_err(|e| e.to_string())
    }

    pub fn list_archived(&self) -> Vec<ArchivedInfo> {
        let mut list: Vec<ArchivedInfo> = list_subfolders(&self.archive_dir())
            .into_iter()
            .filter_map(|folder| {
                let file = self.archive_dir().join(&folder).join("note.md");
                let content = fs::read_to_string(&file).ok()?;
                let archived = get_frontmatter(&content, "archived").unwrap_or_else(|| file_time(&file));
                Some(ArchivedInfo { title: title_of(&content), folder, archived })
            })
            .collect();
        list.sort_by(|a, b| b.archived.cmp(&a.archived));
        list
    }

    pub fn restore_note(&self, folder: &str) -> Result<String, String> {
        let src = self.archive_dir().join(folder);
        let file = src.join("note.md");
        let content = fs::read_to_string(&file).unwrap_or_default();
        fs::write(&file, remove_frontmatter(&content, "archived")).map_err(|e| e.to_string())?;
        let dest = unique_dest(&self.notes_dir(), folder);
        let name = dest.file_name().unwrap().to_string_lossy().to_string();
        fs::rename(&src, dest).map_err(|e| e.to_string())?;
        Ok(name)
    }

    pub fn purge_archive(&self) {
        let cutoff = Utc::now() - chrono::Duration::days(ARCHIVE_DAYS);
        for info in self.list_archived() {
            let archived_at = DateTime::parse_from_rfc3339(&info.archived).map(|d| d.with_timezone(&Utc));
            if let Ok(when) = archived_at {
                if when < cutoff {
                    fs::remove_dir_all(self.archive_dir().join(&info.folder)).ok();
                }
            }
        }
    }

    pub fn save_attachment(&self, folder: &str, name: &str, bytes: &[u8]) -> Result<ImportedFile, String> {
        let dir = self.note_dir(folder).join("attachments");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let dest = unique_dest(&dir, &sanitize(name));
        fs::write(&dest, bytes).map_err(|e| e.to_string())?;
        Ok(self.describe(&dest))
    }

    /// Copies files and folders (recursively) into the note's attachments folder.
    pub fn import_paths(&self, folder: &str, paths: &[PathBuf]) -> Result<Vec<ImportedFile>, String> {
        let dir = self.note_dir(folder).join("attachments");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for src in paths {
            let name = src.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let dest = unique_dest(&dir, &name);
            copy_recursive(src, &dest).map_err(|e| e.to_string())?;
            out.push(self.describe(&dest));
        }
        Ok(out)
    }

    pub fn resolve(&self, folder: &str, rel: &str) -> PathBuf {
        self.note_dir(folder).join(rel)
    }

    fn describe(&self, path: &Path) -> ImportedFile {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        ImportedFile {
            rel: format!("attachments/{}", name),
            is_dir: path.is_dir(),
            is_image: is_image_name(&name),
            name,
        }
    }
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| PathBuf::from(".")).join("Stickies")
}

pub fn load_config() -> Config {
    fs::read_to_string(config_dir().join("config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_config(config: &Config) -> Result<(), String> {
    fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(config_dir().join("config.json"), json).map_err(|e| e.to_string())
}

/// `%OneDrive%\Stickies` when OneDrive is set up, so notes sync with no configuration; else
/// `%USERPROFILE%\Stickies`.
pub fn default_data_dir() -> PathBuf {
    std::env::var_os("OneDrive")
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Stickies")
}

pub fn is_image_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["png", "jpg", "jpeg", "gif", "bmp", "webp", "svg"].iter().any(|ext| lower.ends_with(&format!(".{ext}")))
}

pub fn get_frontmatter(content: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}:");
    frontmatter_lines(content)
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .map(|v| v.trim().trim_matches('"').to_string())
        .filter(|v| !v.is_empty())
}

fn set_frontmatter(content: &str, key: &str, value: &str) -> String {
    let content = remove_frontmatter(content, key);
    let line = format!("{key}: {value}\n");
    if let Some(rest) = content.strip_prefix("---\n") {
        format!("---\n{line}{rest}")
    } else {
        format!("---\n{line}---\n{content}")
    }
}

fn remove_frontmatter(content: &str, key: &str) -> String {
    let prefix = format!("{key}:");
    match content.strip_prefix("---\n").and_then(|rest| rest.split_once("\n---")) {
        Some((fm, body)) => {
            let kept: Vec<&str> = fm.lines().filter(|l| !l.starts_with(prefix.as_str())).collect();
            format!("---\n{}\n---{}", kept.join("\n"), body)
        }
        None => content.to_string(),
    }
}

fn frontmatter_lines(content: &str) -> impl Iterator<Item = &str> {
    content
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---"))
        .map(|(fm, _)| fm)
        .unwrap_or("")
        .lines()
}

fn title_of(content: &str) -> String {
    let body = content
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---"))
        .map(|(_, body)| body)
        .unwrap_or(content);
    body.lines()
        .map(|l| l.trim_start_matches(['#', ' ', '-', '*', '>']).trim())
        .find(|l| !l.is_empty())
        .unwrap_or("(empty note)")
        .chars()
        .take(60)
        .collect()
}

fn now_iso() -> String {
    Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn file_time(path: &Path) -> String {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .map(|t| DateTime::<Local>::from(t).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn list_subfolders(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn unique_dest(dir: &Path, name: &str) -> PathBuf {
    let mut dest = dir.join(name);
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    let mut n = 1;
    while dest.exists() {
        n += 1;
        dest = dir.join(format!("{stem} ({n}){ext}"));
    }
    dest
}

fn sanitize(name: &str) -> String {
    const FORBIDDEN: &str = "<>:\"/\\|?*";
    let cleaned: String = name.chars().map(|c| if FORBIDDEN.contains(c) { '_' } else { c }).collect();
    if cleaned.trim().is_empty() {
        "file".to_string()
    } else {
        cleaned
    }
}

fn copy_recursive(src: &Path, dest: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dest)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_recursive(&entry.path(), &dest.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}
