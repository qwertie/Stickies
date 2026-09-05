//! Native clipboard access for the parts the webview cannot do: file lists (CF_HDROP) in both
//! directions, and bitmap paste from the context menu.

use std::path::PathBuf;

#[cfg(windows)]
mod imp {
    use super::*;
    use clipboard_win::{formats, get_clipboard, Clipboard, Setter};

    pub fn read_files() -> Vec<PathBuf> {
        get_clipboard::<Vec<String>, _>(formats::FileList)
            .map(|v| v.into_iter().map(PathBuf::from).collect())
            .unwrap_or_default()
    }

    pub fn read_text() -> Option<String> {
        get_clipboard::<String, _>(formats::Unicode).ok().filter(|s| !s.is_empty())
    }

    /// Returns the clipboard bitmap as a BMP file image, if any.
    pub fn read_bitmap() -> Option<Vec<u8>> {
        get_clipboard::<Vec<u8>, _>(formats::Bitmap).ok().filter(|b| !b.is_empty())
    }

    pub fn write(text: &str, html: Option<&str>, files: &[PathBuf]) -> Result<(), String> {
        let _clip = Clipboard::new_attempts(10).map_err(|e| e.to_string())?;
        formats::Unicode.write_clipboard(&text).map_err(|e| e.to_string())?;
        if let Some(html) = html {
            let cf_html = formats::Html::new().ok_or("CF_HTML unavailable")?;
            cf_html.write_clipboard(&html).map_err(|e| e.to_string())?;
        }
        if !files.is_empty() {
            let list: Vec<String> = files.iter().map(|p| p.to_string_lossy().to_string()).collect();
            formats::FileList.write_clipboard(&list).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn read_files() -> Vec<PathBuf> {
        Vec::new()
    }
    pub fn read_text() -> Option<String> {
        None
    }
    pub fn read_bitmap() -> Option<Vec<u8>> {
        None
    }
    pub fn write(_: &str, _: Option<&str>, _: &[PathBuf]) -> Result<(), String> {
        Ok(())
    }
}

pub use imp::*;
