import { $, element, icon } from "./shared.js";

const states = {
  running: "Running",
  inputRequested: "Input requested",
  needsAttention: "Needs attention",
  completed: "Session ended",
  failed: "Failed",
  stopped: "Stopped",
};
const rows = new Map();
let selectSession, tabLabel, readExcerpt, submitSummary;
let excerptSession, excerptTrigger;

// Restore focus to the opener on cancel, including in WebKit where clicking a
// button need not focus it. A successful launch gives focus to the new PTY.
function bindDialog(dialog, form, field, submit, cancel, restoreFocus, onSubmit) {
  function cancelDialog() {
    dialog.close();
    restoreFocus()?.focus();
  }
  cancel.onclick = cancelDialog;
  dialog.oncancel = (event) => {
    event.preventDefault();
    cancelDialog();
  };
  dialog.onclose = () => {
    if (!dialog.open) field.value = "";
  };
  field.oninput = () => {
    submit.disabled = !field.value.trim();
  };
  dialog.addEventListener("keydown", (event) => {
    if (!event.isComposing && event.metaKey && event.key === "Enter") {
      event.preventDefault();
      if (!submit.disabled) form.requestSubmit(submit);
    }
  });
  form.onsubmit = (event) => {
    event.preventDefault();
    if (!field.value.trim() || !dialog.open) return;
    dialog.close();
    onSubmit();
  };
}

export function initAgents(
  select,
  label,
  launch,
  preview,
  summarize,
  preferredAgent = () => "codex",
) {
  selectSession = select;
  tabLabel = label;
  readExcerpt = preview;
  submitSummary = summarize;
  const launcher = $("agent-launch");
  const form = $("agent-launch-form");
  const task = $("agent-prompt");
  $("start-agent").onclick = () => {
    form.reset();
    form.elements.agent.value = preferredAgent();
    $("agent-submit").disabled = true;
    launcher.showModal();
    task.focus();
  };
  bindDialog(
    launcher,
    form,
    task,
    $("agent-submit"),
    $("agent-cancel"),
    () => $("start-agent"),
    () => {
      const prompt = task.value.trim();
      task.value = "";
      launch({ agent: form.elements.agent.value, prompt });
    },
  );
  bindDialog(
    $("agent-summary"),
    $("excerpt-form"),
    $("agent-excerpt"),
    $("excerpt-submit"),
    $("excerpt-cancel"),
    () => (excerptTrigger?.isConnected ? excerptTrigger : $("start-agent")),
    () => {
      const excerpt = $("agent-excerpt").value.trim();
      $("agent-excerpt").value = "";
      if (excerptTrigger?.isConnected) excerptTrigger.focus();
      submitSummary({ id: excerptSession, provider: $("excerpt-provider").value, excerpt });
    },
  );
}

function openExcerpt(id, trigger) {
  excerptSession = id;
  excerptTrigger = trigger;
  const field = $("agent-excerpt");
  field.value = (readExcerpt(id) || "").slice(-field.maxLength);
  $("excerpt-provider").value = $("provider").value;
  $("excerpt-submit").disabled = !field.value.trim();
  $("agent-summary").showModal();
  field.focus();
  field.setSelectionRange(0, 0);
}

function newRow(id) {
  const node = element("li", "agent-row");
  const button = element("button", "agent-link");
  button.onclick = () => selectSession(id);
  const text = element("span", "agent-copy");
  const title = element("strong");
  const location = element("small");
  const status = element("small", "agent-status");
  const detail = element("small", "agent-detail");
  const summary = element("small", "agent-summary-text");
  const summarize = element("button", "icon agent-summarize");
  summarize.title = "Summarize activity";
  summarize.setAttribute("aria-label", "Summarize activity");
  summarize.setAttribute("aria-haspopup", "dialog");
  summarize.setAttribute("aria-controls", "agent-summary");
  summarize.append(icon("spark"));
  summarize.onclick = () => {
    if (summarize.getAttribute("aria-disabled") !== "true") openExcerpt(id, summarize);
  };
  text.append(title, location, status, detail, summary);
  button.append(text, icon("arrow"));
  node.append(button, summarize);
  return { node, button, title, location, status, detail, summary, summarize };
}

function setText(node, value) {
  if (node.textContent !== value) node.textContent = value;
}

function updateRow(row, agent) {
  const tab = tabLabel(agent.id);
  const label = agent.managed ? tab.split(" · ", 1)[0] : tab;
  const title = agent.title?.trim() || agent.name;
  const state = agent.state === "blocked" ? "needsAttention" : agent.state;
  const detail = agent.detail?.trim() || "";
  const status = state === "running" && detail ? detail : states[state] || "";
  setText(row.title, title);
  row.title.title = title;
  setText(
    row.location,
    [
      title !== agent.name ? agent.name : "",
      label,
      agent.foreground || agent.managed ? "" : "Background",
    ]
      .filter(Boolean)
      .join(" · "),
  );
  row.button.title = "Go to " + label + (agent.pid ? " · PID " + agent.pid : "");
  row.status.hidden = !status;
  row.status.dataset.state = state || "";
  setText(row.status, status);
  row.detail.hidden =
    !detail ||
    detail === title ||
    detail.replace(/^Run /i, "").toLowerCase() === status.toLowerCase();
  setText(row.detail, detail);
  row.detail.title = detail;
  const summary = agent.summaryPending
    ? "Summarizing…"
    : agent.summaryError || (agent.summary ? "AI summary: " + agent.summary : "");
  row.summary.hidden = !summary;
  row.summary.classList.toggle("error", !!agent.summaryError && !agent.summaryPending);
  setText(row.summary, summary);
  row.summarize.hidden = !!agent.managed || !agent.foreground;
  row.summarize.setAttribute("aria-disabled", String(!!agent.summaryPending));
}

export function renderAgents(agents) {
  const list = $("agents");
  const focused = list.contains(document.activeElement) ? document.activeElement : null;
  const present = new Set();
  let previous = null;
  for (const agent of agents) {
    // Managed sessions keep their identity before and after the PTY process exits.
    const key = agent.managed ? "session-" + agent.id : agent.id + "/" + (agent.pid || agent.name);
    const row = rows.get(key) || newRow(agent.id);
    rows.set(key, row);
    present.add(key);
    updateRow(row, agent);
    const next = previous ? previous.nextElementSibling : list.firstElementChild;
    if (row.node !== next) list.insertBefore(row.node, next);
    previous = row.node;
  }
  for (const [key, row] of rows) {
    if (present.has(key)) continue;
    row.node.remove();
    rows.delete(key);
  }
  setText($("agent-count"), String(agents.length));
  setText($("agent-empty"), "No agents running.");
  $("agent-empty").hidden = agents.length > 0;
  if (focused && (!focused.isConnected || focused.hidden || focused !== document.activeElement)) {
    (focused.isConnected && !focused.hidden
      ? focused
      : list.querySelector(".agent-link") || $("start-agent")
    ).focus({ preventScroll: true });
  }
}
