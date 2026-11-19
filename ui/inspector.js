import { $, element, icon, send } from "./shared.js";

let services,
  showAll = false,
  scanPending = false,
  agentsPending = false,
  lastScan = 0;
let selectSession, tabLabel;

export function initInspector(select, label) {
  selectSession = select;
  tabLabel = label;
  for (const section of document.querySelectorAll("#inspect-panel details")) {
    section.addEventListener("toggle", () => {
      if (section.open && !$("sidebar").hidden && !$("inspect-panel").hidden) refreshInspector();
    });
  }
  $("refresh").onclick = () => refreshInspector(true);
  $("dev-filter").onclick = () => {
    showAll = false;
    renderServices();
  };
  $("all-filter").onclick = () => {
    showAll = true;
    renderServices();
  };
}

export function refreshInspector(force = false) {
  if (!agentsPending && ($("agent-section").open || force)) {
    agentsPending = true;
    send("agents");
  }
  const servicesVisible = $("port-section").open || $("db-section").open;
  if (servicesVisible && (force || Date.now() - lastScan > 10000)) scan();
}
export function receiveServices(payload) {
  services = payload;
  scanPending = false;
  lastScan = Date.now();
  $("refresh").disabled = false;
  renderServices();
}
export function receiveAgents(agents) {
  agentsPending = false;
  $("agent-count").textContent = String(agents.length);
  $("agent-empty").textContent = "No agents running.";
  $("agent-empty").hidden = agents.length !== 0;
  // Preserve keyboard focus across scans when the process list is unchanged.
  const rows = agents.map((agent) => ({ ...agent, label: tabLabel(agent.id) }));
  const fingerprint = JSON.stringify(rows);
  if ($("agents").dataset.fingerprint === fingerprint) return;
  $("agents").dataset.fingerprint = fingerprint;
  $("agents").replaceChildren(
    ...rows.map((agent) => {
      const row = element("li");
      const button = element("button", "agent-link");
      const text = element("span");
      text.append(
        element("strong", "", agent.name),
        element("small", "", agent.label + (agent.foreground ? "" : " · Background")),
      );
      button.append(text, icon("arrow"));
      button.title = "Go to " + agent.label + " · PID " + agent.pid;
      button.onclick = () => selectSession(agent.id);
      row.append(button);
      return row;
    }),
  );
}
function scan() {
  if (scanPending) return;
  scanPending = true;
  $("refresh").disabled = true;
  send("services");
}
function renderServices() {
  if (!services) return;
  const ports = services.ports.filter((port) => showAll || port.kind);
  $("dev-filter").setAttribute("aria-pressed", String(!showAll));
  $("all-filter").setAttribute("aria-pressed", String(showAll));
  $("port-count").textContent = String(ports.length);
  $("ports").replaceChildren(
    ...ports.map((port) => {
      const row = element("li");
      row.append(
        element("span", "port-number", ":" + port.port),
        element("span", "service-name", port.process),
        element(
          "span",
          "service-detail",
          [port.kind, "PID " + port.pid].filter(Boolean).join(" · "),
        ),
      );
      row.title = port.address + ":" + port.port + "  |  " + port.process + "  |  PID " + port.pid;
      if (port.kind) row.lastChild.title = "Common use for this port: " + port.kind;
      return row;
    }),
  );
  $("port-empty").textContent =
    services.portsError ||
    (showAll ? "No listening TCP ports found." : "No development ports in use.");
  $("port-empty").hidden = !!ports.length && !services.portsError;
  $("db-count").textContent = String(services.databases.length);
  $("databases").replaceChildren(
    ...services.databases.map((db) => {
      const row = element("li");
      row.append(
        element("span", "service-name", db.name),
        element("span", "db-state", db.state),
        element("span", "service-detail", db.image),
        element("span", "service-detail", db.ports || "No published ports"),
      );
      return row;
    }),
  );
  $("db-empty").textContent = services.dockerError || "No database containers running.";
  $("db-empty").hidden = !!services.databases.length && !services.dockerError;
}
