import { $, element, icon, send } from "./shared.js";
import { initInspector, refreshInspector, receiveServices, receiveAgents } from "./inspector.js";
import { Activity } from "./activity.js";
import { theme, terminalFont, defaultFontSize } from "./theme.js";
import { initAssistant, receiveAnswer } from "./assistant.js";

const tabs = new Map();
const activity = new Activity((id) => tabs.get(id)?.term, receiveAgents);
let active,
  nextId = 1,
  startPath = "~",
  fontSize = defaultFontSize;
let panel = window.innerWidth > 770 ? "inspector" : null;
let contextTimer, noticeTimer, confirmation, previousPanel;
const current = () => tabs.get(active);
function outputBytes(encoded) {
  if (Uint8Array.fromBase64) return Uint8Array.fromBase64(encoded);
  const raw = atob(encoded);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

function notice(text) {
  clearTimeout(noticeTimer);
  $("notice").textContent = text;
  $("notice").hidden = false;
  noticeTimer = setTimeout(() => {
    $("notice").hidden = true;
  }, 6500);
}
function requestContext() {
  if (current()?.state === "live") send("context", { id: active });
}
function scheduleContext() {
  clearTimeout(contextTimer);
  contextTimer = setTimeout(requestContext, 240);
}
function updateTab(tab) {
  const pathLabel = (tab.path || startPath).split("/").filter(Boolean).pop() || "/";
  const title = tab.title.replace(/(^|:\s+)(?:~?\/)[^\n]*\/([^/]+)$/, "$1$2");
  const name = title || pathLabel;
  tab.label.textContent = name;
  const details = [tab.path];
  if (tab.git) {
    const ref = tab.git.detached ? "HEAD " + tab.git.branch : tab.git.branch;
    const changes = tab.git.dirty === true ? " · Tracked changes" : "";
    details.push(ref + changes);
  }
  tab.button.title = details.join("\n");
  tab.close.setAttribute("aria-label", "Close " + name);
}
function updateSessionState() {
  const tab = current();
  if (!tab) return;
  tab.page.dataset.state = tab.state;
  tab.page.setAttribute("aria-busy", String(tab.state === "opening"));
  $("exit-note").hidden = tab.state !== "ended" && tab.state !== "failed";
  $("exit-note").querySelector("span").textContent = tab.reason || "";
}
function fitActive() {
  const tab = current();
  if (!tab) return;
  tab.fit.fit();
  updateSessionState();
}
function markSelected(tab, selected) {
  tab.page.classList.toggle("selected", selected);
  tab.row.classList.toggle("active", selected);
  tab.button.setAttribute("aria-selected", String(selected));
  tab.button.tabIndex = selected ? 0 : -1;
  tab.page.inert = !selected;
}
function selectTab(id) {
  const tab = tabs.get(id);
  if (!tab) return;
  const previous = current();
  if (previous && previous !== tab) markSelected(previous, false);
  active = id;
  markSelected(tab, true);
  tab.row.scrollIntoView({ block: "nearest", inline: "nearest" });
  tab.term.focus();
  fitActive();
  requestContext();
}
function newTab(task) {
  const from = active;
  const id = nextId++;
  const page = element("div", "terminal-page");
  page.id = "term-" + id;
  page.setAttribute("role", "tabpanel");
  page.setAttribute("aria-labelledby", "tab-" + id);
  $("terminals").append(page);
  const row = element("div", "tab");
  row.dataset.id = String(id);
  const button = element("button", "tab-select");
  button.id = "tab-" + id;
  button.setAttribute("role", "tab");
  button.setAttribute("aria-controls", page.id);
  const label = element("span");
  button.append(icon("terminal"), label);
  button.onclick = () => selectTab(id);
  const close = element("button", "tab-close");
  close.title = "Close tab (⌘W)";
  close.append(icon("close"));
  close.onclick = () => send("close", { id });
  row.append(button, close);
  $("tablist").append(row);
  const term = new Terminal({
    theme,
    fontFamily: terminalFont,
    fontSize,
    lineHeight: 1.2,
    cursorBlink: false,
    cursorStyle: "block",
    cursorInactiveStyle: "outline",
    scrollback: 5000,
    allowProposedApi: false,
    macOptionIsMeta: false,
    allowTransparency: false,
    screenReaderMode: true,
    linkHandler: { activate() {} },
  });
  const fit = new FitAddon.FitAddon();
  term.loadAddon(fit);
  term.open(page);
  const tab = {
    id,
    row,
    page,
    button,
    label,
    close,
    term,
    fit,
    title: task ? task.prompt.trim().replace(/\s+/g, " ").slice(0, 80) : "",
    managed: !!task,
    git: current()?.git,
    path: current()?.path || startPath,
    state: "opening",
    pending: [],
  };
  tabs.set(id, tab);
  term.onData((data) => {
    if (tab.managed) {
      if (data === "\x03" && tab.state === "live") send("stopJob", { id });
      return;
    }
    activity.input(id);
    if (tab.state === "opening") {
      tab.pending.push(data);
      return;
    }
    if (tab.state !== "live") return;
    send("write", { id, data });
    if (data.includes("\r")) scheduleContext();
  });
  term.onBinary((data) => {
    if (!tab.managed && tab.state === "live") send("binary", { id, data: btoa(data) });
  });
  term.onResize(({ cols, rows }) => {
    if (!tab.managed && tab.state === "live") send("resize", { id, cols, rows });
    if (id === active) updateSessionState();
  });
  term.onTitleChange((title) => {
    if (tab.managed) return;
    tab.title = title
      .replace(/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/g, "")
      .slice(0, 180);
    updateTab(tab);
  });
  updateTab(tab);
  selectTab(id);
  if (task) {
    activity.start(id, task);
    send("startJob", { id, from, agent: task.agent, prompt: task.prompt });
  } else {
    send("open", { id, from, cols: term.cols, rows: term.rows });
  }
}
function discard(id) {
  const tab = tabs.get(id);
  if (!tab) return;
  const order = [...tabs.keys()];
  const index = order.indexOf(id);
  tabs.delete(id);
  activity.close(id);
  tab.term.dispose();
  tab.page.remove();
  tab.row.remove();
  if (!tabs.size) {
    send("quit");
    return;
  }
  if (id === active) selectTab(order[index + 1] ?? order[index - 1]);
}
function changeTab(direction) {
  const ids = [...tabs.keys()];
  if (ids.length) selectTab(ids[(ids.indexOf(active) + direction + ids.length) % ids.length]);
}
function setPanel(next, toggle = true) {
  panel = toggle && next === panel ? null : next;
  $("sidebar").hidden = !panel;
  $("inspect-panel").hidden = panel !== "inspector";
  $("assist-panel").hidden = panel !== "assist";
  $("inspector-toggle").setAttribute("aria-pressed", String(panel === "inspector"));
  $("assist-toggle").setAttribute("aria-pressed", String(panel === "assist"));
  if (panel === "inspector") refreshInspector();
  if (panel === "assist") $("question").focus();
  else current()?.term.focus();
  fitActive();
}
function showConfirmation({ id }) {
  confirmation = id ?? "window";
  $("confirm-title").textContent = id ? "Close this tab?" : "Quit Minterm?";
  $("confirm-body").textContent = id
    ? "Closing this tab will stop its running processes."
    : "Your open shell sessions and their processes will close.";
  $("confirm-close").textContent = id ? "Close tab" : "Quit Minterm";
  if (!$("confirm").open) $("confirm").showModal();
  $("cancel").focus();
}
function command(name) {
  const dialog = document.querySelector("dialog[open]");
  if (dialog) {
    if (name === "close") dialog.close();
    return;
  }
  switch (name) {
    case "new":
      newTab();
      break;
    case "close":
      if (active) send("close", { id: active });
      break;
    case "quit":
      send("quit");
      break;
    case "inspector":
    case "assist":
      setPanel(name);
      break;
    case "agents":
      setPanel("inspector", false);
      $("agent-section").open = true;
      refreshInspector();
      ($("agents").querySelector(".agent-link") || $("start-agent")).focus();
      break;
    case "focus":
      if (document.body.classList.toggle("focus-mode")) {
        previousPanel = panel;
        setPanel(null, false);
      } else {
        setPanel(previousPanel, false);
      }
      break;
    case "previous":
      changeTab(-1);
      break;
    case "next":
      changeTab(1);
      break;
    case "larger":
    case "smaller":
    case "reset-size":
      fontSize =
        name === "reset-size"
          ? defaultFontSize
          : Math.max(9, Math.min(28, fontSize + (name === "larger" ? 1 : -1)));
      for (const tab of tabs.values()) tab.term.options.fontSize = fontSize;
      fitActive();
      break;
  }
}
window.minterm = {
  receive(name, data) {
    const tab = tabs.get(data?.id);
    switch (name) {
      case "ready":
        startPath = data.path;
        newTab();
        setPanel(panel, false);
        if (data.vaultError) notice(data.vaultError);
        break;
      case "opened":
        if (tab) {
          tab.path = data.path;
          tab.state = "live";
          updateTab(tab);
          if (tab.pending.length) send("write", { id: tab.id, data: tab.pending.join("") });
          tab.pending.length = 0;
          if (!tab.managed)
            send("resize", { id: tab.id, cols: tab.term.cols, rows: tab.term.rows });
          send("context", { id: tab.id });
          if (active === tab.id) updateSessionState();
        }
        break;
      case "output":
        if (tab) tab.term.write(outputBytes(data.data), () => send("ack", { id: tab.id }));
        break;
      case "exited":
      case "failed":
        if (tab) {
          tab.state = name === "failed" ? "failed" : "ended";
          tab.reason = data.error || data.status;
          if (tab.managed && name === "failed")
            activity.update({ id: tab.id, state: "failed", detail: tab.reason });
          if (active === tab.id) updateSessionState();
        }
        break;
      case "closed":
        discard(data.id);
        break;
      case "context":
        if (tab) {
          Object.assign(tab, { path: data.path, git: data.git });
          updateTab(tab);
          if (active === tab.id) updateSessionState();
        }
        break;
      case "services":
        receiveServices(data);
        break;
      case "agents":
        activity.scan(data);
        break;
      case "jobStatus":
        activity.update(data);
        break;
      case "summary":
        activity.receiveSummary(data);
        break;
      case "notice":
        notice(data);
        break;
      case "menu":
        command(data);
        break;
      case "confirm":
        showConfirmation(data);
        break;
      case "answer":
        receiveAnswer(data);
        break;
    }
  },
};

$("new-tab").onclick = () => newTab();
$("restart").onclick = () => {
  const old = active;
  newTab();
  send("close", { id: old });
};
$("inspector-toggle").onclick = () => setPanel("inspector");
$("assist-toggle").onclick = () => setPanel("assist");
$("cancel").onclick = () => $("confirm").close();
$("confirm").onclose = () => current()?.term.focus();
$("confirm-close").onclick = () => {
  $("confirm").close();
  if (confirmation === "window") send("quit", { force: true });
  else send("close", { id: confirmation, force: true });
};
initInspector(
  (id) => {
    if (window.innerWidth <= 770) setPanel(null, false);
    selectTab(id);
  },
  (id) => {
    const index = [...tabs.keys()].indexOf(id);
    return index < 0 ? "Shell" : "Tab " + (index + 1) + " · " + tabs.get(id).label.textContent;
  },
  (task) => {
    if (window.innerWidth <= 770) setPanel(null, false);
    newTab(task);
  },
  (id) => activity.preview(id),
  (input) => activity.summarize(input),
);
initAssistant();
$("titlebar").addEventListener("mousedown", (e) => {
  if (e.button === 0 && !e.target.closest("button, .tab")) send("drag");
});
document.addEventListener(
  "keydown",
  (e) => {
    if (document.querySelector("dialog[open]") || e.isComposing) return;
    if (e.metaKey && !e.altKey && !e.shiftKey && ["t", "w"].includes(e.key.toLowerCase())) {
      e.preventDefault();
      if (!e.repeat) command(e.key.toLowerCase() === "t" ? "new" : "close");
    } else if (e.metaKey && e.shiftKey && e.code === "KeyA") {
      e.preventDefault();
      command("agents");
    } else if (e.metaKey && e.shiftKey && ["BracketLeft", "BracketRight"].includes(e.code)) {
      e.preventDefault();
      changeTab(e.code === "BracketLeft" ? -1 : 1);
    } else if (
      e.metaKey &&
      !e.altKey &&
      !e.ctrlKey &&
      !e.shiftKey &&
      (/^Digit[1-9]$/.test(e.code) || /^[1-9]$/.test(e.key))
    ) {
      e.preventDefault();
      e.stopPropagation();
      const number = Number(/^Digit[1-9]$/.test(e.code) ? e.code.slice(-1) : e.key);
      const row = $("tablist").children[number - 1];
      if (row) selectTab(Number(row.dataset.id));
    } else if (e.ctrlKey && e.key === "Tab") {
      e.preventDefault();
      changeTab(e.shiftKey ? -1 : 1);
    } else if (e.metaKey && e.key === "Enter" && e.target === $("question")) {
      e.preventDefault();
      $("question-form").requestSubmit();
    } else if (e.key === "Escape" && e.target.closest("#sidebar")) {
      e.preventDefault();
      setPanel(null, false);
    } else if (
      e.target.getAttribute("role") === "tab" &&
      ["ArrowLeft", "ArrowRight"].includes(e.key)
    ) {
      e.preventDefault();
      changeTab(e.key === "ArrowLeft" ? -1 : 1);
      current()?.button.focus();
    }
  },
  true,
);
new ResizeObserver(fitActive).observe($("terminals"));
function refreshVisible() {
  if (document.hidden || !document.hasFocus()) return;
  requestContext();
  if (panel === "inspector") refreshInspector();
}
window.addEventListener("focus", refreshVisible);
setInterval(refreshVisible, 4000);
// The PTY geometry must use the final monospace metrics from its first frame.
document.fonts.load(defaultFontSize + 'px "Martian Mono"').then(
  () => send("ready"),
  () => send("ready"),
);
