import { $, element, send } from "./shared.js";
import { initAgents, renderAgents } from "./agents.js";

let services,
  showAll = false,
  scanPending = false,
  agentsPending = false,
  lastScan = 0;
export function initInspector(select, label, launch, preview, summarize) {
  initAgents(select, label, launch, preview, summarize);
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
  renderAgents(agents);
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
