import { $, element, icon, send } from "./shared.js";
import { initAgents, renderAgents } from "./agents.js";

let confirmListener;
let services,
  showAll = false,
  scanPending = false,
  rescan = false,
  stopping = false,
  agentsPending = false,
  lastScan = 0;
export function inspectorState(visible) {
  return { visible, agentsOpen: $("agent-section").open, portsOpen: $("port-section").open, showAll };
}

export function restoreInspector(state) {
  $("agent-section").open = state.agentsOpen ?? true;
  $("port-section").open = state.portsOpen ?? false;
  showAll = state.showAll ?? false;
  renderServices();
}

export function initInspector(select, label, launch, preferredAgent, refreshPreferences, save, confirmStop) {
  confirmListener = confirmStop;
  initAgents(select, label, launch, preferredAgent, refreshPreferences);
  for (const section of document.querySelectorAll("#inspect-panel details")) {
    section.addEventListener("toggle", () => {
      save();
      if (section.open && !$("sidebar").hidden && !$("inspect-panel").hidden) refreshInspector();
    });
  }
  $("refresh").onclick = () => refreshInspector(true);
  $("dev-filter").onclick = () => {
    showAll = false;
    save();
    renderServices();
  };
  $("all-filter").onclick = () => {
    showAll = true;
    save();
    renderServices();
  };
}

export function refreshInspector(force = false) {
  if (!agentsPending && ($("agent-section").open || force)) {
    agentsPending = true;
    send("agents");
  }
  if ($("port-section").open && (force || Date.now() - lastScan > 10000)) scan();
}
export function receiveServices(payload) {
  services = payload;
  scanPending = false;
  lastScan = Date.now();
  $("refresh").disabled = false;
  renderServices();
  if (rescan) {
    rescan = false;
    scan();
  }
}

export function stopListener(listener) {
  stopping = true;
  $("ports").querySelectorAll("button").forEach((button) => { button.disabled = true; });
  send("stopListener", { listener });
}

export function listenerStopped() {
  stopping = false;
  if (scanPending) rescan = true;
  else scan();
}

export function receiveAgents(agents) {
  agentsPending = false;
  renderAgents(agents);
}
function scan() {
  if (scanPending) return;
  scanPending = true;
  $("refresh").disabled = true;
  send("services");
}
function renderServices() {
  $("dev-filter").setAttribute("aria-pressed", String(!showAll));
  $("all-filter").setAttribute("aria-pressed", String(showAll));
  if (!services) return;
  const entries = services.entries.filter((entry) => showAll || entry.kind);
  $("port-count").textContent = String(entries.length);
  $("ports").replaceChildren(...entries.map(serviceRow));
  const errors = [services.portsError, services.dockerError].filter(Boolean);
  $("service-errors").textContent = errors.join(" ");
  $("service-errors").hidden = !errors.length;
  $("port-empty").textContent = showAll ? "No listening ports found." : "No development ports in use.";
  $("port-empty").hidden = entries.length > 0 || errors.length > 0;
}

function serviceRow(entry) {
  const row = element("li");
  if (entry.type === "container") {
    const binding = entry.bindings[0];
    const metadata = ["Docker", entry.state, entry.image].filter(Boolean).join(" · ");
    const mapping = binding ? entry.ports : ["No host port", entry.ports].filter(Boolean).join(" · ");
    row.append(
      element("span", "port-number", binding ? ":" + binding.start : "—"),
      element("span", "service-name", entry.name),
      element("span", "service-detail", metadata),
      element("span", "service-detail", mapping.replaceAll("->", " → ")),
    );
  } else {
    const details = [entry.kind, "PID " + entry.pid].filter(Boolean).join(" · ");
    row.append(
      element("span", "port-number", ":" + entry.port),
      element("span", "service-name", entry.process),
      element("span", "service-detail", details),
    );
    const close = element("button", "icon port-close");
    close.type = "button";
    close.title = `Stop ${entry.process} on port ${entry.port}`;
    close.setAttribute("aria-label", close.title);
    close.disabled = stopping;
    close.append(icon("close"));
    close.onclick = () => confirmListener(entry);
    row.append(close);
    row.title = entry.address + ":" + entry.port + "  |  " + entry.process + "  |  " + details;
  }
  return row;
}
