import { $, agentLabel, send } from "./shared.js";

// Locate the prompt in terminal cells, including wide glyphs and soft wraps.
// Custom prompts without a recognizable terminator keep the real cursor as their anchor.
function promptAnchor(term) {
  const buffer = term.buffer.active;
  const cursorRow = buffer.baseY + buffer.cursorY;
  if (cursorRow >= buffer.viewportY + term.rows) {
    return { col: buffer.cursorX, row: cursorRow, lastRow: cursorRow };
  }
  let firstRow = cursorRow;
  while (firstRow > buffer.viewportY && buffer.getLine(firstRow)?.isWrapped) firstRow--;
  let lastRow = cursorRow;
  while (lastRow < buffer.viewportY + term.rows - 1 && buffer.getLine(lastRow + 1)?.isWrapped) {
    lastRow++;
  }
  let fallback;
  for (let row = firstRow; row <= cursorRow; row++) {
    const line = buffer.getLine(row);
    if (!line) continue;
    const limit = row === cursorRow ? buffer.cursorX : term.cols;
    for (let col = 0; col < limit; col++) {
      const glyph = line.getCell(col)?.getChars();
      if (!["❯", "›", ">"].includes(glyph)) continue;
      const next = line.getCell(col + 1)?.getChars();
      if (next && next !== " ") continue;
      const anchor = { col, row, lastRow };
      if (glyph === "❯" || glyph === "›") return anchor;
      // A redirection within an unfinished shell command is not its prompt.
      if (!fallback && !line.translateToString(true, 0, col).trim().includes(" ")) {
        fallback = anchor;
      }
    }
  }
  return fallback || { col: buffer.cursorX, row: cursorRow, lastRow };
}

export class CommandMode {
  constructor(currentTab, openAgent, toggle, notice) {
    this.currentTab = currentTab;
    this.openAgent = openAgent;
    this.notice = notice;
    this.profiles = new Set();
    this.bindings = new Map();
    this.agent = "codex";
    this.shortcut = "";
    this.newTabMode = "terminal";
    this.listeners = [];
    const field = $("command-input");
    $("terminals").addEventListener("click", (event) => {
      const session = this.session;
      if (session?.page.contains(event.target) && !session.term.hasSelection()) {
        field.focus({ preventScroll: true });
      }
    });
    field.oninput = () => {
      const tab = currentTab();
      if (tab?.mode !== "compose") return;
      tab.draft = field.value;
      const buffer = tab.base.term.buffer.active;
      if (buffer.viewportY !== buffer.baseY) tab.base.term.scrollToBottom();
      this.layout();
    };
    this.bindLaunchKeys(field, toggle);
    $("command-form").onsubmit = (event) => {
      event.preventDefault();
      this.submit();
    };
  }

  bindLaunchKeys(field, toggle) {
    const currentTab = this.currentTab;
    // Keep repeats from a held launch key out of the newly focused CLI.
    const held = new Set();
    const physicalKey = (event) => event.code || event.key;
    const cancelChord = () => {
      this.enterTab = null;
    };
    field.addEventListener("blur", () => {
      cancelChord();
      this.queuedSubmit = null;
    });
    field.addEventListener("compositionstart", cancelChord);
    window.addEventListener("blur", () => {
      cancelChord();
      held.clear();
    });
    document.addEventListener("keydown", (event) => {
      if (!held.has(physicalKey(event))) return;
      if (event.repeat) {
        event.preventDefault();
        event.stopImmediatePropagation();
      } else {
        held.delete(physicalKey(event));
      }
    }, true);
    document.addEventListener("keyup", (event) => {
      if (held.delete(physicalKey(event))) {
        event.preventDefault();
        event.stopImmediatePropagation();
      } else if (event.key === "Enter" && this.enterTab) {
        const tab = this.enterTab;
        cancelChord();
        event.preventDefault();
        if (!event.isComposing && document.activeElement === field && currentTab() === tab) {
          this.submit();
        }
      }
    }, true);
    field.onkeydown = (event) => {
      if (event.isComposing || event.keyCode === 229) {
        cancelChord();
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        cancelChord();
        toggle();
        return;
      }
      if (event.shiftKey || event.metaKey || event.altKey) {
        cancelChord();
        return;
      }
      if (event.ctrlKey) {
        cancelChord();
        // Some keyboard layouts deliver the ASCII control character to WebKit.
        let letter = event.key.toUpperCase();
        if (/^Key[A-Z]$/.test(event.code) && /^[\x01-\x1a]$/.test(event.key)) {
          letter = String.fromCharCode(64 + event.key.charCodeAt(0));
        }
        const shortcut = "Ctrl+" + letter;
        if (this.bindings.has(shortcut)) {
          event.preventDefault();
          if (!event.repeat) {
            held.add(physicalKey(event));
            this.submit(shortcut);
          }
        }
        return;
      }
      if (event.key === "Enter") {
        event.preventDefault();
        if (event.repeat) return;
        if (this.hasReturnShortcuts || this.refreshing) {
          this.enterTab = currentTab();
          this.enterKey = physicalKey(event);
        } else {
          held.add(physicalKey(event));
          this.submit();
        }
      } else if (this.enterTab) {
        const shortcut = "Enter+" + event.key.toUpperCase();
        const tab = this.enterTab;
        cancelChord();
        if (tab === currentTab() && /^[a-z]$/i.test(event.key) && (this.bindings.has(shortcut) || this.refreshing)) {
          event.preventDefault();
          held.add(this.enterKey);
          held.add(physicalKey(event));
          this.submit(shortcut);
        }
      }
    };
  }

  canLaunch(agent) {
    if (!this.error && this.profiles.has(agent)) return true;
    this.notice(this.error || "Agent command unavailable. Check Settings.");
    return false;
  }

  // Coalesce focus changes and launcher opens. Text entry stays live while
  // the host reads commands that may have changed in another terminal.
  refresh() {
    if (!this.refreshing) {
      this.refreshing = new Promise((resolve) => {
        this.didRefresh = resolve;
      });
      send("agentPreferences");
    }
    return this.refreshing;
  }

  async submit(shortcut) {
    const tab = this.currentTab();
    if (tab?.mode !== "compose") return;
    const field = $("command-input");
    let prompt = field.value;
    if (this.refreshing) {
      const queued = { tab, prompt };
      this.queuedSubmit = queued;
      await this.refreshing;
      if (this.queuedSubmit !== queued || this.currentTab() !== tab || field.value !== prompt) return;
      this.queuedSubmit = null;
    }
    let agent = this.agent;
    if (shortcut) {
      agent = this.bindings.get(shortcut);
      if (!agent) {
        this.notice(this.error || "No agent command uses " + shortcut + ".");
        return;
      }
    } else {
      const prefix = /^(\S+)(?:\s|$)/u.exec(prompt);
      if (prefix && this.profiles.has(prefix[1])) {
        agent = prefix[1];
        prompt = prompt.slice(prefix[0].length);
      }
    }
    if (this.openAgent(tab, agent, prompt.trim() ? prompt : "")) tab.draft = "";
  }

  preferences(value) {
    if (!value) return;
    this.profiles.clear();
    this.bindings.clear();
    this.hasReturnShortcuts = false;
    const accessibleShortcuts = ["Enter", "Shift+Enter", "Escape"];
    for (const { name, shortcut } of value.agentCommands || []) {
      this.profiles.add(name);
      if (!shortcut) continue;
      const letter = shortcut.slice(-1);
      this.bindings.set("Ctrl+" + letter, name);
      accessibleShortcuts.push("Control+" + letter);
      if (shortcut.startsWith("Enter+")) {
        this.hasReturnShortcuts = true;
        this.bindings.set(shortcut, name);
      }
    }
    $("command-input").setAttribute("aria-keyshortcuts", accessibleShortcuts.join(" "));
    this.agent = value.agent || "codex";
    this.error = value.agentCommandsError;
    this.shortcut = value.shortcut || "";
    this.newTabMode = value.newTabMode === "agent" ? "agent" : "terminal";
    this.refreshing = null;
    this.didRefresh?.();
    this.didRefresh = null;
    this.sync();
  }

  attach(session) {
    if (this.session === session) return;
    for (const listener of this.listeners) listener.dispose();
    this.listeners = [];
    this.session?.term.element?.style.removeProperty("transform");
    this.session?.page.classList.remove("composing");
    (session?.page || $("terminals")).append($("command-form"));
    this.enterTab = null;
    this.session = session;
    this.shift = 0;
    cancelAnimationFrame(this.frame);
    this.frame = 0;
    if (!session) return;
    session.page.classList.add("composing");
    session.term.scrollToBottom();
    const schedule = () => {
      if (this.frame) return;
      this.frame = requestAnimationFrame(() => {
        this.frame = 0;
        this.layout();
      });
    };
    const { term } = session;
    this.listeners.push(
      term.onRender(schedule),
      term.onCursorMove(schedule),
      term.onScroll(schedule),
      term.onResize(schedule),
    );
  }

  layout() {
    if (!this.session) return;
    const { page, term } = this.session;
    const form = $("command-form");
    const field = $("command-input");
    const grid = term.element.querySelector(".xterm-screen").getBoundingClientRect();
    if (!grid.width || !grid.height) return;
    const cellWidth = grid.width / term.cols;
    const cellHeight = grid.height / term.rows;
    const buffer = term.buffer.active;
    const anchor = promptAnchor(term);
    let row = anchor.row - buffer.viewportY;
    let col = anchor.col;
    // xterm may represent the pending wrap as a cursor one column past the grid.
    if (col >= term.cols) {
      col = 0;
      row++;
    }
    const atBottom = buffer.viewportY === buffer.baseY;
    const outOfView = row < 0 || (row >= term.rows && !atBottom);
    form.style.opacity = outOfView ? "0" : "";
    form.style.pointerEvents = outOfView ? "none" : "";
    if (outOfView) {
      this.shift = 0;
      term.element.style.removeProperty("transform");
      return;
    }
    const bounds = page.getBoundingClientRect();
    const x = col * cellWidth;
    const fieldWrap = term.cols - col < 8;
    const fieldLeft = fieldWrap ? -x : 2 * cellWidth;
    form.style.left = grid.left - bounds.left + x + "px";
    form.style.width = grid.width - x + "px";
    form.style.setProperty("--command-cell", cellWidth + "px");
    form.style.setProperty("--command-line", cellHeight + "px");
    form.style.setProperty("--command-wrap-left", -x + "px");
    form.style.setProperty("--command-wrap-width", grid.width + "px");
    const maskRows = Math.max(0, anchor.lastRow - anchor.row);
    form.style.setProperty("--command-mask-height", maskRows * cellHeight + "px");
    form.style.fontSize = term.options.fontSize + "px";
    field.style.left = fieldLeft + "px";
    field.style.top = (fieldWrap ? cellHeight : 0) + "px";
    field.style.width = Math.max(cellWidth, grid.width - x - fieldLeft) + "px";
    // Measure wrapping in WebKit itself. Long drafts scroll within the textarea.
    field.style.height = cellHeight + "px";
    const maxRows = Math.max(1, term.rows - Number(fieldWrap));
    const height = Math.min(Math.max(cellHeight, field.scrollHeight), maxRows * cellHeight);
    field.style.height = height + "px";
    const depth = (row + Number(fieldWrap)) * cellHeight + height;
    const shift = Math.max(0, depth - grid.height);
    // Slide only the presentation when the last terminal line needs space. The
    // real shell stays at its original size with its own line editor untouched.
    form.style.top = grid.top + this.shift - bounds.top + row * cellHeight - shift + "px";
    term.element.style.transform = shift ? "translateY(-" + shift + "px)" : "";
    this.shift = shift;
  }

  sync() {
    const tab = this.currentTab();
    const composing = tab?.mode === "compose";
    const agentShown = composing || tab?.mode === "agent";
    const toggle = $("command-toggle");
    toggle.disabled = !tab;
    const companion = tab?.companion;
    let tooltip = "Agent mode";
    if (agentShown) tooltip = "Return to shell";
    else if (companion && ["opening", "live"].includes(companion.state)) {
      tooltip = "Return to " + agentLabel(companion.agent);
    }
    toggle.title = this.shortcut ? tooltip + " (" + this.shortcut + ")" : tooltip;
    toggle.setAttribute("aria-label", tooltip);
    toggle.setAttribute("aria-pressed", String(!!agentShown));
    const tabName = this.newTabMode === "agent" ? "New agent tab" : "New terminal tab";
    $("new-tab").title = tabName + " (⌘T)";
    $("new-tab").setAttribute("aria-label", tabName);
    $("command-form").hidden = !composing;
    this.attach(composing ? tab.base : undefined);
    const field = $("command-input");
    const draft = composing ? tab.draft || "" : "";
    if (field.value !== draft) field.value = draft;
    if (!composing) return;
    const name = agentLabel(this.agent);
    $("command-marker").setAttribute("aria-label", name);
    field.setAttribute("aria-label", "Initial task for " + name);
    this.layout();
  }
}
