// Work around xterm.js #6078 in the bundled 6.0.0 renderer. Its fallback for
// keyCode 229 can send the entire textarea when WebKit edits it in place.
// Keep the textarea intact for composition and VoiceOver.
export function fixTerminalInput(term) {
  const helper = term._core?._compositionHelper;
  const { element, textarea } = term;
  if (!element || !textarea || typeof helper?._handleAnyTextareaChanges !== "function") return;

  let timer, before;
  function flush() {
    if (timer === undefined) return;
    clearTimeout(timer);
    timer = undefined;
    if (helper._isComposing || !element.isConnected) return;

    const after = textarea.value;
    const text = insertedText(before, after);
    helper._dataAlreadySent = text;
    // Preserve xterm's Backspace behavior for a deletion.
    const data = text || (before.length > after.length ? "\x7f" : "");
    if (data) helper._coreService.triggerDataEvent(data, true);
  }

  helper._handleAnyTextareaChanges = () => {
    if (timer !== undefined) return;
    before = textarea.value;
    timer = setTimeout(flush, 0);
  };

  // xterm handles keydown on the textarea in capture phase. Settle its pending
  // edit on the parent before a normal key can write to or clear the textarea.
  element.addEventListener("keydown", (event) => {
    if (event.target === textarea && event.keyCode !== 229) flush();
  }, true);
  textarea.addEventListener("compositionstart", flush, true);
}

function insertedText(before, after) {
  if (after.startsWith(before)) return after.slice(before.length);
  // Compare code points so a replacement cannot split a surrogate pair.
  const previous = Array.from(before);
  const current = Array.from(after);
  let start = 0;
  while (start < previous.length && start < current.length && previous[start] === current[start]) {
    start++;
  }
  let oldEnd = previous.length;
  let newEnd = current.length;
  while (oldEnd > start && newEnd > start && previous[oldEnd - 1] === current[newEnd - 1]) {
    oldEnd--;
    newEnd--;
  }
  return current.slice(start, newEnd).join("");
}
