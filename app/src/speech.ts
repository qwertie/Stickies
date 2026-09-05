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
