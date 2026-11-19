export const $ = (id) => document.getElementById(id);
export const send = (action, values = {}) =>
  window.ipc.postMessage(JSON.stringify({ action, ...values }));
const paths = {
  plus: "M12 5v14M5 12h14",
  close: "m6 6 12 12M18 6 6 18",
  terminal: "m4 6 6 6-6 6m9 0h6",
  panel: "M3 5h18v14H3zM14 5v14",
  spark: "m12 3 2.3 6.7L21 12l-6.7 2.3L12 21l-2.3-6.7L3 12l6.7-2.3z",
  folder: "M3 6h6l2 3h10v10H3z",
  ports: "M4 6v5m4-5v5M3 11h6v2a3 3 0 0 1-6 0zm3 5v3h8m2-15v5m4-5v5m-5 0h6v2a3 3 0 0 1-6 0zm3 5v5",
  branch: "M7 8v9m0-6c0 3 10 3 10-3",
  database:
    "M4 6c0-4.7 16-4.7 16 0s-16 4.7-16 0zm0 0v12c0 4.7 16 4.7 16 0V6M4 12c0 4.7 16 4.7 16 0",
  refresh: "M20 10a8 8 0 0 0-14-4L3 9m0-5v5h5m-4 5a8 8 0 0 0 14 4l3-3m0 5v-5h-5",
  agents: "m4 7 5 5-5 5m7-10 5 5-5 5m6 0h4",
  chevron: "m7 10 5 5 5-5",
  settings: "M4 6h9m4 0h3M4 12h3m4 0h9M4 18h11m4 0h1M13 3v6m-6 0v6m8 0v6",
  key: "M14.5 4a5.5 5.5 0 1 1-3.7 9.6L6 18.4V21H3v-4l6.6-6.6A5.5 5.5 0 0 1 14.5 4zM16 8h.01",
  arrow: "M5 12h14m-6-6 6 6-6 6",
};
export function icon(name) {
  const holder = document.createElement("i");
  holder.setAttribute("aria-hidden", "true");
  const branchNodes =
    name === "branch"
      ? '<circle cx="7" cy="5" r="2"/><circle cx="7" cy="19" r="2"/><circle cx="17" cy="6" r="2"/>'
      : "";
  // Paths are static application assets. Session, service and model text use textContent.
  holder.innerHTML =
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round"><path d="' +
    paths[name] +
    '"/>' +
    branchNodes +
    "</svg>";
  return holder;
}
document.querySelectorAll("[data-icon]").forEach((el) => el.replaceWith(icon(el.dataset.icon)));
export function element(tag, className, text) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
