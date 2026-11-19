export const $ = (id) => document.getElementById(id);
const agentNames = {
  codex: "Codex",
  claude: "Claude Code",
  opencode: "OpenCode",
};
export const agentLabel = (id) => Object.hasOwn(agentNames, id) ? agentNames[id] : id;
export const send = (action, values = {}) =>
  window.ipc.postMessage(JSON.stringify({ action, ...values }));
const paths = {
  plus: "M12 5v14M5 12h14",
  robot: "M12 2v3M5 6h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2Zm3 5v2m8-2v2m-7 3h6",
  close: "m6 6 12 12M18 6 6 18",
  terminal: "m4 6 6 6-6 6m9 0h6",
  panel: "M3 5h18v14H3zM14 5v14",
  ports: "M4 6v5m4-5v5M3 11h6v2a3 3 0 0 1-6 0zm3 5v3h8m2-15v5m4-5v5m-5 0h6v2a3 3 0 0 1-6 0zm3 5v5",
  refresh: "M20 10a8 8 0 0 0-14-4L3 9m0-5v5h5m-4 5a8 8 0 0 0 14 4l3-3m0 5v-5h-5",
  agents: "m4 7 5 5-5 5m7-10 5 5-5 5m6 0h4",
  chevron: "m7 10 5 5 5-5",
  arrow: "M5 12h14m-6-6 6 6-6 6",
};
export function icon(name) {
  const holder = document.createElement("i");
  holder.dataset.icon = name;
  holder.setAttribute("aria-hidden", "true");
  // Paths are static application assets. Session and service text use textContent.
  holder.innerHTML =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round"><path d="' +
    paths[name] +
    '"/></svg>';
  return holder;
}
document.querySelectorAll("[data-icon]").forEach((el) => el.replaceWith(icon(el.dataset.icon)));
export function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
