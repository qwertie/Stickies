//! Speech-to-text through Windows.Media.SpeechRecognition (dictation topic). Recognized phrases
//! are emitted to the requesting window as `dictation` events.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Windows returns SPERR_SPEECH_PRIVACY_POLICY_NOT_ACCEPTED until "Online speech recognition" is
/// switched on under Settings > Privacy & security > Speech. The UI recognises this prefix.
pub const PRIVACY_ERROR: &str = "SPEECH_PRIVACY";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictationEvent {
    pub text: Option<String>,
    pub error: Option<String>,
    pub ended: bool,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::Mutex;
    use windows::core::HSTRING;
    use windows::Foundation::TypedEventHandler;
    use windows::Media::SpeechRecognition::*;

    pub struct Session {
        recognizer: SpeechRecognizer,
        session: SpeechContinuousRecognitionSession,
    }

    static ACTIVE: Mutex<Option<Session>> = Mutex::new(None);

    pub fn start(app: AppHandle, label: String) -> Result<(), String> {
        stop();
        let recognizer = SpeechRecognizer::new().map_err(|e| e.to_string())?;
        let constraint = SpeechRecognitionTopicConstraint::Create(
            SpeechRecognitionScenario::Dictation,
            &HSTRING::from("dictation"),
        )
        .map_err(|e| e.to_string())?;
        recognizer.Constraints().map_err(|e| e.to_string())?.Append(&constraint).map_err(|e| e.to_string())?;
        let compiled = recognizer
            .CompileConstraintsAsync()
            .map_err(|e| e.to_string())?
            .get()
            .map_err(|e| e.to_string())?;
        if compiled.Status().map_err(|e| e.to_string())? != SpeechRecognitionResultStatus::Success {
            return Err("Speech recognition constraints failed to compile".into());
        }
        let session = recognizer.ContinuousRecognitionSession().map_err(|e| e.to_string())?;

        let (app1, label1) = (app.clone(), label.clone());
        session
            .ResultGenerated(&TypedEventHandler::new(
                move |_, args: windows::core::Ref<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
                    if let Some(args) = args.as_ref() {
                        let text = args.Result()?.Text()?.to_string();
                        emit(&app1, &label1, Some(text), None, false);
                    }
                    Ok(())
                },
            ))
            .map_err(|e| e.to_string())?;
        let (app2, label2) = (app.clone(), label.clone());
        session
            .Completed(&TypedEventHandler::new(
                move |_, args: windows::core::Ref<SpeechContinuousRecognitionCompletedEventArgs>| {
                    let status = args.as_ref().and_then(|a| a.Status().ok());
                    let error = status
                        .filter(|s| *s != SpeechRecognitionResultStatus::Success)
                        .map(|s| format!("{s:?}"));
                    emit(&app2, &label2, None, error, true);
                    Ok(())
                },
            ))
            .map_err(|e| e.to_string())?;
        session.StartAsync().map_err(|e| e.to_string())?.get().map_err(|e| {
            if e.code().0 as u32 == 0x8004_5509 {
                PRIVACY_ERROR.to_string()
            } else {
                e.to_string()
            }
        })?;
        *ACTIVE.lock().unwrap() = Some(Session { recognizer, session });
        Ok(())
    }

    pub fn stop() {
        if let Some(active) = ACTIVE.lock().unwrap().take() {
            if let Ok(op) = active.session.StopAsync() {
                op.get().ok();
            }
            drop(active.recognizer);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;
    pub fn start(_: AppHandle, _: String) -> Result<(), String> {
        Err("Dictation is only available on Windows".into())
    }
    pub fn stop() {}
}

fn emit(app: &AppHandle, label: &str, text: Option<String>, error: Option<String>, ended: bool) {
    app.emit_to(label, "dictation", DictationEvent { text, error, ended }).ok();
}

pub use imp::{start, stop};
