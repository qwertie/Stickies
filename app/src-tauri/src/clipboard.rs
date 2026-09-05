//! Native clipboard access for the parts the webview cannot do: file lists in both directions,
//! and image paste from the context menu.
//!
//! Windows uses clipboard-win so text, HTML and a file list can all be placed at once (which is
//! what makes "copy chips like text" work). Other platforms use arboard, which sets one format at
//! a time, so a copy containing attachments puts only the file list on the clipboard there.

use std::path::PathBuf;

/// Encoded image bytes plus a file extension for saving them.
pub type ClipImage = (Vec<u8>, &'static str);

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

    pub fn read_image() -> Option<ClipImage> {
        get_clipboard::<Vec<u8>, _>(formats::Bitmap).ok().filter(|b| !b.is_empty()).map(|b| (b, "bmp"))
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
    use arboard::Clipboard;

    pub fn read_files() -> Vec<PathBuf> {
        Clipboard::new().and_then(|mut c| c.get().file_list()).unwrap_or_default()
    }

    pub fn read_text() -> Option<String> {
        Clipboard::new().and_then(|mut c| c.get_text()).ok().filter(|s| !s.is_empty())
    }

    pub fn read_image() -> Option<ClipImage> {
        let image = Clipboard::new().and_then(|mut c| c.get_image()).ok()?;
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, image.width as u32, image.height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().ok()?;
            writer.write_image_data(&image.bytes).ok()?;
        }
        Some((out, "png"))
    }

    pub fn write(text: &str, html: Option<&str>, files: &[PathBuf]) -> Result<(), String> {
        let mut clip = Clipboard::new().map_err(|e| e.to_string())?;
        if !files.is_empty() {
            clip.set().file_list(files).map_err(|e| e.to_string())
        } else if let Some(html) = html {
            clip.set().html(html, Some(text)).map_err(|e| e.to_string())
        } else {
            clip.set_text(text).map_err(|e| e.to_string())
        }
    }
}

pub use imp::*;
