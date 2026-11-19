import { $, agentNames } from "./shared.js";

export class CommandMode {
  constructor(currentTab, openAgent, toggle) {
    this.currentTab = currentTab;
    this.agent = "codex";
    this.shortcut = "";
    this.newTabMode = "terminal";
    const field = $("command-input");
    field.oninput = () => {
      const tab = currentTab();
      if (tab?.mode === "compose") tab.draft = field.value;
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
        if (!event.repeat && !$("command-submit").disabled) $("command-form").requestSubmit();
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
    toggle.setAttribute("aria-pressed", String(!!agentShown));
    const tabName = this.newTabMode === "agent" ? "New agent tab" : "New terminal tab";
    $("new-tab").title = tabName + " (⌘T)";
    $("new-tab").setAttribute("aria-label", tabName);
    $("command-form").hidden = !composing;
    const field = $("command-input");
    const draft = composing ? tab.draft || "" : "";
    if (field.value !== draft) field.value = draft;
    if (!composing) return;
    $("command-agent").textContent = agentNames[this.agent];
    $("command-dir").textContent = tab.base.cwdKnown
      ? tab.base.path
      : tab.base.state === "opening"
        ? "Opening…"
        : "Folder unavailable";
    $("command-dir").title = tab.base.cwdKnown ? tab.base.path : "";
    $("command-submit").disabled = !tab.base.cwdKnown;
  }
}
