import { $, agentNames } from "./shared.js";

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
  constructor(currentTab, openAgent, toggle) {
    this.currentTab = currentTab;
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
    field.onkeydown = (event) => {
      if (event.isComposing) return;
      if (event.key === "Escape") {
        event.preventDefault();
        toggle();
      } else if (
        event.key === "Enter" &&
        !event.shiftKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        !event.altKey
      ) {
        event.preventDefault();
        if (!event.repeat) $("command-form").requestSubmit();
      }
    };
    $("command-form").onsubmit = (event) => {
      event.preventDefault();
      const tab = currentTab();
      if (tab?.mode !== "compose" || !tab.base.cwdKnown) return;
      const prompt = field.value.trim();
      tab.draft = "";
      openAgent(tab, this.agent, prompt);
    };
  }

  preferences(value) {
    if (!value) return;
    this.agent = Object.hasOwn(agentNames, value.agent) ? value.agent : "codex";
    this.shortcut = value.shortcut || "";
    this.newTabMode = value.newTabMode === "agent" ? "agent" : "terminal";
    this.sync();
  }

  attach(session) {
    if (this.session === session) return;
    for (const listener of this.listeners) listener.dispose();
    this.listeners = [];
    this.session?.term.element?.style.removeProperty("transform");
    this.session?.page.classList.remove("composing");
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
      tooltip = "Return to " + agentNames[companion.agent];
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
    const name = agentNames[this.agent];
    $("command-marker").setAttribute("aria-label", name);
    field.setAttribute("aria-label", "Initial task for " + name);
    this.layout();
  }
}
