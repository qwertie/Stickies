//! Speech-to-text. On Windows this delegates to the built-in voice typing panel (Win+H), which
//! types into whatever control has focus, so the note's editor receives the words directly.
//!
//! Why not Windows.Media.SpeechRecognition: its dictation mode relies on the legacy Windows Speech
//! Recognition backend, which Microsoft retired; on current Windows 11 it either reports "speech
//! privacy policy not accepted" or hangs without ever producing a result, while Win+H works.

#[cfg(windows)]
pub fn toggle_voice_typing() -> Result<(), String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_H, VK_LWIN,
    };

    fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { Default::default() },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    let inputs = [key(VK_LWIN, false), key(VK_H, false), key(VK_H, true), key(VK_LWIN, true)];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == inputs.len() as u32 {
        Ok(())
    } else {
        Err("Windows did not accept the Win+H keystroke".into())
    }
}

#[cfg(not(windows))]
pub fn toggle_voice_typing() -> Result<(), String> {
    Err("Dictation is not available on this platform yet".into())
}
