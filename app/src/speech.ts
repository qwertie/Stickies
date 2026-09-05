import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

interface DictationEvent {
  text: string | null;
  hypothesis: string | null;
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

export interface DictationCallbacks {
  onText(text: string): void;
  onHypothesis(text: string): void;
  onEnd(error: string | null): void;
}

/**
 * Starts Windows dictation for this window; returns a function that stops it. Stopping keeps
 * listening for events until the engine reports the session ended, so the last phrase spoken
 * before Stop still gets inserted.
 */
export async function startDictation(label: string, cb: DictationCallbacks): Promise<() => void> {
  let unlisten: UnlistenFn | undefined;
  let finished = false;
  const finish = (error: string | null) => {
    if (!finished) {
      finished = true;
      unlisten?.();
      unlisten = undefined;
      cb.onEnd(error);
    }
  };
  unlisten = await listen<DictationEvent>('dictation', (e) => {
    if (e.payload.hypothesis) {
      cb.onHypothesis(e.payload.hypothesis);
    }
    if (e.payload.text) {
      cb.onText(e.payload.text);
    }
    if (e.payload.ended) {
      finish(e.payload.error);
    }
  });
  try {
    await invoke('start_dictation', { label });
  } catch (err) {
    finish(String(err));
  }
  return () => {
    void invoke('stop_dictation');
    window.setTimeout(() => finish(null), 3000); // in case the engine never reports completion
  };
}
