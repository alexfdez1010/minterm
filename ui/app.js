import { $, agentLabel, element, icon, send } from "./shared.js";
import { setAgentChoices } from "./agents.js";
import {
  initInspector, refreshInspector, receiveServices, receiveAgents,
  inspectorState, restoreInspector, stopListener, listenerStopped,
} from "./inspector.js";
import { Activity } from "./activity.js";
import { theme, terminalFont, defaultFontSize, loadTerminalFonts } from "./theme.js";
import { CommandMode } from "./command.js";
import { fixTerminalInput } from "./terminal-input.js";

const tabs = new Map();
const sessions = new Map();
const activity = new Activity((id) => sessions.get(id)?.term, receiveAgents);
let active,
  nextId = 1,
  startPath,
  fontSize = defaultFontSize;
let panel = window.innerWidth > 770 ? "inspector" : null;
let contextTimer, noticeTimer, confirmation, previousPanel, savedInspector;
const currentTab = () => tabs.get(active);
const visibleSession = (tab) => (tab?.mode === "agent" ? tab.companion : tab?.base);
const current = () => visibleSession(currentTab());
const commandMode = new CommandMode(currentTab, openInteractive, toggleCommand, notice);
function focusSession() {
  if (currentTab()?.mode === "compose") $("command-input").focus();
  else current()?.term.focus();
}
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
  const session = current();
  if (session?.state === "live") send("context", { id: session.id });
}
function scheduleContext() {
  clearTimeout(contextTimer);
  contextTimer = setTimeout(requestContext, 240);
}
function updateTab(tab) {
  const session = visibleSession(tab);
  if (tab.iconKind !== session.kind) {
    tab.button.firstElementChild.replaceWith(
      icon(session.kind === "agent" ? "robot" : "terminal"),
    );
    tab.iconKind = session.kind;
  }
  const pathLabel = (session.path || startPath).split("/").filter(Boolean).pop() || "/";
  const title = session.title.replace(/(^|:\s+)(?:~?\/)[^\n]*\/([^/]+)$/, "$1$2");
  const name =
    session.kind === "agent"
      ? session.taskTitle || agentLabel(session.agent) + " · " + pathLabel
      : (active === tab.id && session.lastCommand) || title || pathLabel;
  tab.label.textContent = name;
  const details = [session.path];
  if (session.kind === "agent") details.push(agentLabel(session.agent));
  if (session.git) {
    const changes = session.git.dirty === true ? " · Tracked changes" : "";
    details.push((session.git.detached ? "HEAD " : "") + session.git.branch + changes);
  }
  tab.button.title = details.join("\n");
  tab.close.setAttribute("aria-label", "Close " + name);
}
function updateSessionState() {
  const session = current();
  if (!session) return;
  session.page.dataset.state = session.state;
  session.page.setAttribute("aria-busy", String(session.state === "opening"));
  $("exit-note").hidden = session.state !== "ended" && session.state !== "failed";
  $("exit-note").querySelector("span").textContent = session.reason || "";
  $("restart").textContent = session.kind === "agent" ? "Restart agent" : "Open new shell";
}
function fitActive() {
  const session = current();
  if (!session) return;
  session.fit.fit();
  updateSessionState();
}
function markSelected(tab, selected) {
  const visible = visibleSession(tab);
  tab.button.setAttribute("aria-controls", visible.page.id);
  for (const session of [tab.base, tab.companion]) {
    if (!session) continue;
    session.page.classList.toggle("selected", selected && session === visible);
    session.page.inert = !selected || session !== visible;
  }
  tab.row.classList.toggle("active", selected);
  tab.button.setAttribute("aria-selected", String(selected));
  tab.button.tabIndex = selected ? 0 : -1;
}
function selectTab(id) {
  const tab = tabs.get(id);
  if (!tab) return;
  const previous = currentTab();
  active = id;
  if (previous && previous !== tab) {
    markSelected(previous, false);
    updateTab(previous);
  }
  markSelected(tab, true);
  updateTab(tab);
  tab.row.scrollIntoView({ block: "nearest", inline: "nearest" });
  commandMode.sync();
  focusSession();
  fitActive();
  requestContext();
}
function setMode(tab, mode) {
  if (mode === "compose" && tab.mode !== mode) commandMode.refresh();
  tab.mode = mode;
  tab.base.term.options.disableStdin = mode === "compose";
  selectTab(tab.id);
}
function toggleCommand() {
  const tab = currentTab();
  if (!tab) return;
  if (tab.mode !== "shell") {
    setMode(tab, "shell");
  } else if (tab.companion && ["opening", "live"].includes(tab.companion.state)) {
    setMode(tab, "agent");
  } else {
    setMode(tab, "compose");
  }
}
function createSession(id, tabId, kind, source) {
  const page = element("div", "terminal-page");
  page.id = "term-" + id;
  page.dataset.kind = kind;
  page.setAttribute("role", "tabpanel");
  page.setAttribute("aria-labelledby", "tab-" + tabId);
  $("terminals").append(page);
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
  fixTerminalInput(term);
  const session = {
    id,
    tabId,
    kind,
    page,
    term,
    fit,
    title: "",
    git: source?.git,
    path: source?.path || startPath,
    cwdKnown: false,
    state: "opening",
    pending: [],
  };
  sessions.set(id, session);
  term.onData((data) => {
    // Cursor reports and arrow navigation do not answer an approval prompt.
    if (/[\r\n\x03\x04]/.test(data) || /^(?:y|n|\x1b)$/i.test(data)) activity.input(id);
    if (session.state === "opening") {
      session.pending.push(data);
      return;
    }
    if (session.state !== "live") return;
    send("write", { id, data });
    if (data.includes("\r")) scheduleContext();
  });
  term.onBinary((data) => {
    if (session.state === "live") send("binary", { id, data: btoa(data) });
  });
  term.onResize(({ cols, rows }) => {
    if (session.state === "live") send("resize", { id, cols, rows });
    if (current() === session) updateSessionState();
  });
  if (kind === "shell") {
    // Matches the private OSC emitted by the shell integration.
    term.parser.registerOscHandler(6973, (command) => {
      const preview = command
        .replace(/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/g, " ")
        .trim()
        .replace(/\s+/g, " ");
      if (preview) {
        session.lastCommand = preview.slice(0, 80);
        const tab = currentTab();
        if (current() === session) {
          updateTab(tab);
          tab.row.scrollIntoView({ block: "nearest", inline: "nearest" });
        }
      }
      return true;
    });
  }
  term.onTitleChange((title) => {
    session.title = title
      .replace(/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/g, "")
      .slice(0, 180);
    const tab = tabs.get(tabId);
    if (tab && visibleSession(tab) === session) updateTab(tab);
  });
  return session;
}
function newTab(task, from = current()?.id, mode = commandMode.newTabMode) {
  if (!startPath) return;
  if (tabs.size && (task || mode === "agent") && !commandMode.canLaunch(task?.agent || commandMode.agent)) return;
  const id = nextId++;
  const row = element("div", "tab");
  row.dataset.id = String(id);
  const button = element("button", "tab-select");
  button.id = "tab-" + id;
  button.setAttribute("role", "tab");
  const label = element("span");
  button.append(icon("terminal"), label);
  button.onclick = () => selectTab(id);
  const close = element("button", "tab-close");
  close.title = "Close tab (⌘W)";
  close.append(icon("close"));
  close.onclick = () => send("close", { id });
  row.append(button, close);
  $("tablist").append(row);
  const base = createSession(id, id, "shell", sessions.get(from));
  const tab = { id, row, button, label, close, base, companion: null, mode: "shell", draft: "" };
  tabs.set(id, tab);
  selectTab(id);
  send("open", { id, from, cols: base.term.cols, rows: base.term.rows });
  if (task || mode === "agent")
    openInteractive(tab, task?.agent || commandMode.agent, task?.prompt || "");
  return tab;
}
function dispatchInteractive(tab) {
  const session = tab.companion;
  if (!session || !tab.base.cwdKnown || session.initialPrompt === undefined) return;
  const prompt = session.initialPrompt;
  delete session.initialPrompt;
  send("openAgent", {
    id: session.id,
    from: tab.base.id,
    agent: session.agent,
    prompt,
    cols: session.term.cols,
    rows: session.term.rows,
  });
}
function openInteractive(tab, agent, prompt) {
  if (tab.companion && ["opening", "live"].includes(tab.companion.state)) {
    setMode(tab, "agent");
    return true;
  }
  if (!commandMode.canLaunch(agent)) return false;
  if (tab.companion) {
    disposeSession(tab.companion);
  }
  const companion = createSession(nextId++, tab.id, "agent", tab.base);
  companion.agent = agent;
  companion.initialPrompt = prompt;
  companion.taskTitle = prompt.trim().replace(/\s+/g, " ").slice(0, 80);
  activity.start(companion.id, { agent, prompt });
  tab.companion = companion;
  setMode(tab, "agent");
  dispatchInteractive(tab);
  return true;
}
function disposeSession(session) {
  sessions.delete(session.id);
  activity.close(session.id);
  session.term.dispose();
  session.page.remove();
}
function discard(id) {
  const tab = tabs.get(id);
  if (!tab) return;
  const order = [...tabs.keys()];
  const index = order.indexOf(id);
  tabs.delete(id);
  disposeSession(tab.base);
  if (tab.companion) disposeSession(tab.companion);
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
function saveInspector() {
  if (savedInspector === undefined) return;
  const state = inspectorState(panel === "inspector");
  const serialized = JSON.stringify(state);
  if (serialized === savedInspector) return;
  savedInspector = serialized;
  send("saveInspector", { state });
}
function setPanel(next, toggle = true) {
  panel = toggle && next === panel ? null : next;
  $("sidebar").hidden = !panel;
  $("inspect-panel").hidden = panel !== "inspector";
  $("inspector-toggle").setAttribute("aria-pressed", String(panel === "inspector"));
  saveInspector();
  if (panel === "inspector") refreshInspector();
  focusSession();
  fitActive();
}
function confirmStop(listener) {
  confirmation = listener;
  $("confirm-title").textContent = `Stop ${listener.process}?`;
  $("confirm-body").textContent = `This will force quit PID ${listener.pid}, which is listening on port ${listener.port}.`;
  $("confirm-close").textContent = "Stop process";
  $("confirm").showModal();
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
    case "newTerminalTab":
      newTab(undefined, undefined, "terminal");
      break;
    case "newAgentTab":
      newTab(undefined, undefined, "agent");
      break;
    case "new":
      newTab();
      break;
    case "commandMode":
      if (window.innerWidth <= 770 && panel) setPanel(null, false);
      toggleCommand();
      break;
    case "close":
      if (active) send("close", { id: active });
      break;
    case "quit":
      send("quit");
      break;
    case "inspector":
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
      for (const session of sessions.values()) session.term.options.fontSize = fontSize;
      fitActive();
      break;
  }
}
function applyAgentPreferences(prefs) {
  commandMode.preferences(prefs);
  setAgentChoices(prefs?.agentCommands || [], commandMode.agent, commandMode.error);
}
window.minterm = {
  receive(name, data) {
    const session = sessions.get(data?.id);
    const tab = tabs.get(session?.tabId);
    switch (name) {
      case "ready":
        startPath = data.path;
        applyAgentPreferences(data.preferences);
        const state = data.preferences.inspector;
        restoreInspector(state ?? {});
        if (state?.visible != null) panel = state.visible ? "inspector" : null;
        savedInspector = JSON.stringify(inspectorState(panel === "inspector"));
        newTab();
        setPanel(panel, false);
        break;
      case "opened":
        if (session) {
          session.path = data.path;
          session.cwdKnown = true;
          session.state = "live";
          session.page.dataset.state = session.state;
          if (session.pending.length)
            send("write", { id: session.id, data: session.pending.join("") });
          session.pending.length = 0;
          send("resize", { id: session.id, cols: session.term.cols, rows: session.term.rows });
          send("context", { id: session.id });
          if (session === tab.base) dispatchInteractive(tab);
          else activity.update({ id: session.id, state: "running", detail: "Session open" });
          updateTab(tab);
          if (tab.id === active) {
            commandMode.sync();
            updateSessionState();
          }
        }
        break;
      case "output":
        if (session)
          session.term.write(outputBytes(data.data), () => {
            send("ack", { id: data.id });
            if (session.kind === "agent") activity.observe(session.id);
          });
        break;
      case "exited":
      case "failed":
        if (session) {
          session.state = name === "failed" ? "failed" : "ended";
          session.reason = data.error || data.status;
          session.page.dataset.state = session.state;
          if (session.kind === "agent") {
            const success = name === "exited" && (data.success ?? data.status === "Success");
            // The host can reap the child while xterm still has a final write queued.
            session.term.write("", () => activity.finish(session.id, success, session.reason));
          }
          if (session === tab.base && !session.cwdKnown && tab.companion) {
            tab.companion.reason = session.reason;
            tab.companion.state = "failed";
            delete tab.companion.initialPrompt;
            activity.finish(tab.companion.id, false, "Could not open the source shell");
          }
          if (tab.id === active) {
            commandMode.sync();
            updateSessionState();
          }
        }
        break;
      case "closed":
        discard(data.id);
        break;
      case "context":
        if (session) {
          Object.assign(session, { path: data.path, git: data.git });
          updateTab(tab);
          if (active === tab.id && tab.mode === "compose") commandMode.sync();
        }
        break;
      case "services":
        receiveServices(data);
        break;
      case "listenerStopped":
        listenerStopped();
        if (data) notice(data);
        break;
      case "agents":
        activity.scan(data);
        break;
      case "notice":
        notice(data);
        break;
      case "menu":
        command(data);
        break;
      case "preferences":
        applyAgentPreferences(data);
        break;
      case "confirm":
        showConfirmation(data);
        break;
    }
  },
};

$("new-tab").onclick = () => newTab();
const tabOptions = $("new-tab-options");
for (const mode of ["terminal", "agent"]) {
  $("new-" + mode + "-tab").onclick = () => {
    tabOptions.open = false;
    newTab(undefined, undefined, mode);
  };
}
document.addEventListener("pointerdown", (event) => {
  if (tabOptions.open && !tabOptions.contains(event.target)) tabOptions.open = false;
});
tabOptions.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && tabOptions.open) {
    event.preventDefault();
    tabOptions.open = false;
    tabOptions.querySelector("summary").focus();
  }
});
$("restart").onclick = () => {
  const tab = currentTab();
  if (!tab) return;
  if (current().kind === "agent") {
    if (!tab.base.cwdKnown) {
      setMode(tab, "shell");
      return;
    }
    openInteractive(tab, current().agent, "");
  } else {
    newTab(undefined, tab.id, "terminal");
    send("close", { id: tab.id });
  }
};
$("inspector-toggle").onclick = () => setPanel("inspector");
$("command-toggle").onclick = () => command("commandMode");
$("cancel").onclick = () => $("confirm").close();
$("confirm").onclose = () => focusSession();
$("confirm-close").onclick = () => {
  $("confirm").close();
  if (confirmation === "window") send("quit", { force: true });
  else if (typeof confirmation === "number") send("close", { id: confirmation, force: true });
  else stopListener(confirmation);
};
initInspector(
  (id) => {
    const session = sessions.get(id);
    const tab = tabs.get(session?.tabId);
    if (!tab) return;
    if (window.innerWidth <= 770) setPanel(null, false);
    setMode(tab, session === tab.companion ? "agent" : "shell");
  },
  (id) => {
    const tab = tabs.get(sessions.get(id)?.tabId);
    if (!tab) return "Shell";
    return "Tab " + ([...tabs.keys()].indexOf(tab.id) + 1) + " · " + tab.label.textContent;
  },
  (task) => {
    if (window.innerWidth <= 770) setPanel(null, false);
    newTab(task);
  },
  () => commandMode.agent,
  () => commandMode.refresh(),
  saveInspector,
  confirmStop,
);
$("titlebar").addEventListener("mousedown", (e) => {
  if (e.button === 0 && !e.target.closest("button, summary, .tab")) send("drag");
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
    } else if (e.key === "Escape" && e.target.closest("#sidebar")) {
      e.preventDefault();
      setPanel(null, false);
    } else if (
      e.target.getAttribute("role") === "tab" &&
      ["ArrowLeft", "ArrowRight"].includes(e.key)
    ) {
      e.preventDefault();
      changeTab(e.key === "ArrowLeft" ? -1 : 1);
      currentTab()?.button.focus();
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
window.addEventListener("focus", () => {
  commandMode.refresh();
  refreshVisible();
});
setInterval(refreshVisible, 4000);
// The PTY geometry must use the final monospace metrics from its first frame.
loadTerminalFonts().then(() => send("ready"));
