import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

interface DictationEvent {
  text: string | null;
  error: string | null;
  ended: boolean;
}

/** Text-to-speech through the webview's speechSynthesis (Windows voices). */
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

/** Starts Windows dictation for this window; returns a function that stops it. */
export async function startDictation(
  label: string,
  onText: (text: string) => void,
  onEnd: (error: string | null) => void,
): Promise<() => void> {
  let unlisten: UnlistenFn | undefined;
  const stop = () => {
    unlisten?.();
    unlisten = undefined;
    void invoke('stop_dictation');
  };
  unlisten = await listen<DictationEvent>('dictation', (e) => {
    if (e.payload.text) {
      onText(e.payload.text);
    }
    if (e.payload.ended) {
      unlisten?.();
      unlisten = undefined;
      onEnd(e.payload.error);
    }
  });
  try {
    await invoke('start_dictation', { label });
  } catch (err) {
    stop();
    onEnd(String(err));
  }
  return stop;
}
