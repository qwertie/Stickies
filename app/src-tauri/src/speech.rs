//! Speech-to-text through Windows.Media.SpeechRecognition (dictation topic). Recognized phrases
//! are emitted to the requesting window as `dictation` events. Other platforms have no engine yet,
//! so the event plumbing is unused there.
#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Windows returns SPERR_SPEECH_PRIVACY_POLICY_NOT_ACCEPTED until "Online speech recognition" is
/// switched on under Settings > Privacy & security > Speech. The UI recognises this prefix.
pub const PRIVACY_ERROR: &str = "SPEECH_PRIVACY";

#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DictationEvent {
    /// A finished phrase to insert.
    pub text: Option<String>,
    /// Words recognised so far in the phrase still being spoken (display only).
    pub hypothesis: Option<String>,
    pub error: Option<String>,
    pub ended: bool,
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;
    use windows::core::HSTRING;
    use windows::Foundation::{TimeSpan, TypedEventHandler};
    use windows::Media::SpeechRecognition::*;

    /// Kept alive while dictating; the recognizer owns the session and its event handlers.
    pub struct Session {
        _recognizer: SpeechRecognizer,
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
        let status = compiled.Status().map_err(|e| e.to_string())?;
        if status != SpeechRecognitionResultStatus::Success {
            return Err(format!("constraints failed to compile: {}", status_name(status)));
        }
        let session = recognizer.ContinuousRecognitionSession().map_err(|e| e.to_string())?;
        // Default is 20 s of silence, after which the session quietly ends.
        session
            .SetAutoStopSilenceTimeout(TimeSpan { Duration: 10 * 60 * 10_000_000 })
            .map_err(|e| e.to_string())?;

        let (app1, label1) = (app.clone(), label.clone());
        recognizer
            .HypothesisGenerated(&TypedEventHandler::new(
                move |_, args: windows::core::Ref<SpeechRecognitionHypothesisGeneratedEventArgs>| {
                    if let Some(args) = args.as_ref() {
                        let text = args.Hypothesis()?.Text()?.to_string();
                        emit(&app1, &label1, DictationEvent { hypothesis: Some(text), ..Default::default() });
                    }
                    Ok(())
                },
            ))
            .map_err(|e| e.to_string())?;
        let (app2, label2) = (app.clone(), label.clone());
        session
            .ResultGenerated(&TypedEventHandler::new(
                move |_, args: windows::core::Ref<SpeechContinuousRecognitionResultGeneratedEventArgs>| {
                    if let Some(args) = args.as_ref() {
                        let result = args.Result()?;
                        let text = result.Text()?.to_string();
                        log::info!("dictation result ({:?}): {text}", result.Confidence());
                        if !text.is_empty() {
                            emit(&app2, &label2, DictationEvent { text: Some(text), ..Default::default() });
                        }
                    }
                    Ok(())
                },
            ))
            .map_err(|e| e.to_string())?;
        let (app3, label3) = (app.clone(), label.clone());
        session
            .Completed(&TypedEventHandler::new(
                move |_, args: windows::core::Ref<SpeechContinuousRecognitionCompletedEventArgs>| {
                    let status = args.as_ref().and_then(|a| a.Status().ok());
                    log::info!("dictation session completed: {:?}", status.map(status_name));
                    let error = match status {
                        None
                        | Some(SpeechRecognitionResultStatus::Success)
                        | Some(SpeechRecognitionResultStatus::UserCanceled)
                        | Some(SpeechRecognitionResultStatus::TimeoutExceeded)
                        | Some(SpeechRecognitionResultStatus::PauseLimitExceeded) => None,
                        Some(other) => Some(status_name(other).to_string()),
                    };
                    emit(&app3, &label3, DictationEvent { error, ended: true, ..Default::default() });
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
        log::info!("dictation started for {label}");
        *ACTIVE.lock().unwrap() = Some(Session { _recognizer: recognizer, session });
        Ok(())
    }

    /// Stops listening. StopAsync (unlike CancelAsync) still delivers results for audio already
    /// captured, so the recognizer is kept alive a moment for those events to arrive.
    pub fn stop() {
        if let Some(active) = ACTIVE.lock().unwrap().take() {
            if let Ok(op) = active.session.StopAsync() {
                op.get().ok();
            }
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(1500));
                drop(active);
            });
        }
    }

    fn status_name(status: SpeechRecognitionResultStatus) -> &'static str {
        match status {
            SpeechRecognitionResultStatus::Success => "success",
            SpeechRecognitionResultStatus::TopicLanguageNotSupported => "the speech language is not supported for dictation",
            SpeechRecognitionResultStatus::GrammarLanguageMismatch => "grammar language mismatch",
            SpeechRecognitionResultStatus::GrammarCompilationFailure => "grammar compilation failure",
            SpeechRecognitionResultStatus::AudioQualityFailure => "audio quality too low",
            SpeechRecognitionResultStatus::UserCanceled => "stopped",
            SpeechRecognitionResultStatus::TimeoutExceeded => "silence timeout",
            SpeechRecognitionResultStatus::PauseLimitExceeded => "pause limit exceeded",
            SpeechRecognitionResultStatus::NetworkFailure => "network failure (dictation uses Microsoft's online service)",
            SpeechRecognitionResultStatus::MicrophoneUnavailable => {
                "microphone unavailable (check Settings > Privacy & security > Microphone, including 'Let desktop apps access your microphone')"
            }
            _ => "unknown failure",
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

fn emit(app: &AppHandle, label: &str, event: DictationEvent) {
    app.emit_to(label, "dictation", event).ok();
}

pub use imp::{start, stop};
