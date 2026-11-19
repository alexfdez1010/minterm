import { $, agentNames, element, icon } from "./shared.js";

const states = {
  running: "Running",
  inputRequested: "Input requested",
  needsAttention: "Needs attention",
  completed: "Session ended",
  failed: "Failed",
  stopped: "Stopped",
};
const rows = new Map();
let selectSession, tabLabel, choiceEdited;

export function setAgentChoices(commands, preferred, error) {
  const field = $("agent-choice");
  const selected = $("agent-launch").open && choiceEdited ? field.value : preferred;
  const choices = [...Object.entries(agentNames), ...commands.map(({ name }) => [name, name])];
  field.replaceChildren(...choices.map(([value, label]) => new Option(label, value)));
  field.value = selected;
  field.setCustomValidity(error || "");
  $("agent-submit").disabled = !field.value || !$("agent-prompt").value.trim();
}

export function initAgents(select, label, launch, preferredAgent, refreshPreferences) {
  selectSession = select;
  tabLabel = label;
  const dialog = $("agent-launch");
  const form = $("agent-launch-form");
  const task = $("agent-prompt");
  const submit = $("agent-submit");
  const opener = $("start-agent");
  let opening;
  opener.onclick = () => {
    choiceEdited = false;
    opening = { ready: refreshPreferences() };
    form.reset();
    form.elements.agent.value = preferredAgent();
    submit.disabled = true;
    dialog.showModal();
    task.focus();
  };
  const cancel = () => {
    dialog.close();
    opener.focus();
  };
  $("agent-cancel").onclick = cancel;
  dialog.oncancel = (event) => {
    event.preventDefault();
    cancel();
  };
  dialog.onclose = () => {
    if (!dialog.open) {
      opening = null;
      task.value = "";
    }
  };
  form.elements.agent.onchange = () => {
    choiceEdited = true;
    task.oninput();
  };
  task.oninput = () => {
    submit.disabled = !task.value.trim() || !form.elements.agent.value;
  };
  dialog.onkeydown = (event) => {
    if (!event.isComposing && event.metaKey && event.key === "Enter") {
      event.preventDefault();
      if (!submit.disabled) form.requestSubmit(submit);
    }
  };
  form.onsubmit = async (event) => {
    event.preventDefault();
    const submitted = opening;
    if (!submitted) return;
    await submitted.ready;
    if (!dialog.open || opening !== submitted || !form.reportValidity()) return;
    const prompt = task.value.trim();
    if (!prompt) return;
    dialog.close();
    launch({ agent: form.elements.agent.value, prompt });
  };
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
  text.append(title, location, status, detail);
  button.append(text, icon("arrow"));
  node.append(button);
  return { node, button, title, location, status, detail };
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
