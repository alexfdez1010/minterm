import { $, icon, send } from "./shared.js";

const providerName = () => ($("provider").value === "anthropic" ? "Anthropic" : "OpenAI");
let pending = false,
  answeringProvider;

function updateForm() {
  $("ask").textContent = pending ? "Asking " + answeringProvider + "…" : "Ask " + providerName();
  if (!pending) $("ask").append(icon("arrow"));
  $("ask").disabled = pending || !$("question").value.trim();
  $("provider").disabled = pending;
}
export function initAssistant() {
  $("settings").onclick = () => send("settings");
  $("provider").onchange = updateForm;
  $("question").oninput = updateForm;
  $("question-form").onsubmit = (event) => {
    event.preventDefault();
    const question = $("question").value.trim();
    if (!question || pending) return;
    pending = true;
    answeringProvider = providerName();
    $("reply").hidden = true;
    updateForm();
    send("ask", { provider: $("provider").value, question });
  };
  updateForm();
}
export function receiveAnswer(data) {
  pending = false;
  $("reply").hidden = false;
  $("reply-title").textContent = answeringProvider;
  $("answer").textContent = data.error || data.text;
  $("answer").classList.toggle("error", !!data.error);
  updateForm();
}
