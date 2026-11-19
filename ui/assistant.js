import { $, icon, send } from "./shared.js";
import { terminalExcerpt } from "./activity.js";

const providerName = () => $("provider").selectedOptions[0].text;
let pending = false,
  answeringProvider,
  excerptSource,
  requestSource,
  requestQuestion;

export function updateContextAction(tab) {
  $("assistant-context").textContent = tab?.term.hasSelection()
    ? "Explain selection"
    : "Explain recent output";
  $("assistant-context").disabled = !tab || pending;
}

function updateForm() {
  $("ask").textContent = pending ? "Asking " + answeringProvider + "…" : "Ask " + providerName();
  if (!pending) $("ask").append(icon("arrow"));
  $("ask").disabled = pending || !$("question").value.trim();
  $("provider").disabled = pending;
}
export function initAssistant(current, notice, openDraft) {
  $("settings").onclick = () => send("settings");
  $("provider").onchange = updateForm;
  $("question").oninput = updateForm;
  $("assistant-context").onclick = () => {
    const tab = current();
    if (!tab) return;
    const field = $("assistant-excerpt");
    const selection = tab.term.getSelection().trim();
    const excerpt = selection
      ? selection.slice(0, field.maxLength)
      : terminalExcerpt(tab.term).slice(-field.maxLength);
    if (!excerpt) {
      notice("There is no terminal output to explain yet.");
      return;
    }
    excerptSource = tab.id;
    field.value = excerpt;
    $("assistant-context-editor").hidden = false;
    $("assistant-sharing").textContent = "Only the question and this excerpt are sent.";
    if (!$("question").value.trim())
      $("question").value = "Why did this happen, and what should I try next?";
    updateForm();
    field.focus();
    field.setSelectionRange(0, 0);
  };
  $("remove-excerpt").onclick = () => {
    $("assistant-context-editor").hidden = true;
    $("assistant-excerpt").value = "";
    $("assistant-sharing").textContent = "Only the question is sent.";
    excerptSource = undefined;
    $("question").focus();
  };
  $("question-form").onsubmit = (event) => {
    event.preventDefault();
    const question = $("question").value.trim();
    if (!question || pending) return;
    const excerpt = $("assistant-context-editor").hidden ? "" : $("assistant-excerpt").value.trim();
    requestSource = excerpt ? excerptSource : current()?.id;
    requestQuestion = question;
    pending = true;
    answeringProvider = providerName();
    $("reply").hidden = true;
    updateForm();
    updateContextAction(current());
    send("ask", {
      provider: $("provider").value,
      question: excerpt ? question + "\n\nTerminal excerpt\n" + excerpt : question,
    });
  };
  $("assistant-followup").onclick = () => {
    const goal = "Follow up on this request in the project.\n\n";
    const advice = "\n\nAssistant's suggestion\n" + $("answer").textContent;
    const task = goal + requestQuestion.slice(0, 1500) + advice;
    openDraft(requestSource, task.slice(0, $("command-input").maxLength));
  };
  updateForm();
}
export function receiveAnswer(data) {
  pending = false;
  $("reply").hidden = false;
  $("reply-title").textContent = answeringProvider;
  $("answer").textContent = data.error || data.text;
  $("answer").classList.toggle("error", !!data.error);
  $("assistant-followup").hidden = !!data.error || !data.text;
  $("assistant-context").disabled = false;
  updateForm();
}
