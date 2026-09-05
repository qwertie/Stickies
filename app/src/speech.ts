import { invoke } from '@tauri-apps/api/core';

/** Text-to-speech through the webview's speechSynthesis (system voices). */
export function speak(text: string) {
  stopSpeaking();
  const trimmed = text.trim();
  if (trimmed) {
    speechSynthesis.speak(new SpeechSynthesisUtterance(trimmed));
  }
}

export function stopSpeaking() {
  speechSynthesis.cancel();
}

/**
 * Speech-to-text: opens the operating system's voice typing (Win+H on Windows), which types into
 * the focused editor. The caller must focus the editor first.
 */
export async function toggleDictation(): Promise<string | null> {
  try {
    await invoke('toggle_dictation');
    return null;
  } catch (err) {
    return String(err);
  }
}
