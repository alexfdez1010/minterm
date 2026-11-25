import { $, element, icon, send } from "./shared.js";

export class Projects {
  constructor(openDirectory, restoreFocus) {
    this.dialog = $("projects-dialog");
    this.search = $("project-search");
    this.list = $("project-list");
    this.entries = [];
    this.loaded = false;
    this.removing = null;
    $("projects-toggle").onclick = () => this.open();
    $("projects-close").onclick = () => this.dialog.close();
    this.dialog.addEventListener("close", restoreFocus);
    this.search.addEventListener("input", () => this.render());
    this.list.addEventListener("click", (event) => {
      const button = event.target.closest("button[data-path]");
      if (!button) return;
      if (button.classList.contains("project-remove")) {
        button.focus();
        this.remove(button.dataset.path);
        return;
      }
      this.dialog.close();
      openDirectory(button.dataset.path);
    });
    this.dialog.addEventListener("keydown", (event) => {
      const buttons = [...this.list.querySelectorAll(".project-link")];
      if (event.key === "Escape") {
        event.preventDefault();
        this.dialog.close();
      } else if (event.key === "Enter" && event.target === this.search && buttons.length) {
        event.preventDefault();
        buttons[0].click();
      } else if (["ArrowDown", "ArrowUp"].includes(event.key)) {
        event.preventDefault();
        const index = buttons.indexOf(document.activeElement.closest("li")?.querySelector(".project-link"));
        if (event.key === "ArrowUp" && index <= 0) this.search.focus();
        else buttons[Math.min(buttons.length - 1, index + (event.key === "ArrowDown" ? 1 : -1))]?.focus();
      }
    });
  }

  preferences(prefs) {
    const shortcut = prefs?.projectsShortcut || "";
    const button = $("projects-toggle");
    button.title = shortcut ? "Open project (" + shortcut + ")" : "Open project";
    if (shortcut) {
      button.setAttribute("aria-keyshortcuts", shortcut
        .replace("⌃", "Control+").replace("⌥", "Alt+").replace("⇧", "Shift+").replace("⌘", "Meta+"));
    } else button.removeAttribute("aria-keyshortcuts");
  }

  open() {
    if (this.dialog.open) return;
    this.loaded = false;
    this.search.value = "";
    this.list.replaceChildren();
    $("projects-empty").textContent = "Loading projects…";
    $("projects-empty").hidden = false;
    $("projects-error").hidden = true;
    this.dialog.showModal();
    this.search.focus();
    send("projects");
  }

  receive({ entries, error }) {
    const focused = document.activeElement;
    const restore = this.dialog.open && this.list.contains(focused);
    const focusedPath = focused.dataset.path;
    const index = restore ? [...this.list.children].indexOf(focused.closest("li")) : -1;
    const selector = focused.classList.contains("project-remove") ? ".project-remove" : ".project-link";
    this.removing = null;
    this.entries = entries;
    this.loaded = true;
    $("projects-error").textContent = error || "";
    $("projects-error").hidden = !error;
    this.render();
    if (restore) {
      const row = [...this.list.children].find((row) => row.querySelector(".project-link").dataset.path === focusedPath)
        || this.list.children[Math.min(index, this.list.children.length - 1)];
      (row?.querySelector(selector) || this.search).focus();
    }
  }

  closeSelection() {
    const button = document.activeElement.closest("button[data-path]");
    if (button && this.list.contains(button)) this.remove(button.dataset.path);
    else this.dialog.close();
  }

  remove(path) {
    if (this.removing) return;
    this.removing = path;
    send("removeProject", { path });
  }

  render() {
    if (!this.loaded) return;
    const query = this.search.value.trim().toLocaleLowerCase();
    const entries = this.entries.filter((entry) =>
      (entry.name + " " + entry.displayPath).toLocaleLowerCase().includes(query));
    this.list.replaceChildren(...entries.map((entry) => {
      const row = element("li");
      const button = element("button", "project-link");
      button.type = "button";
      button.dataset.path = entry.path;
      button.title = "Open terminal in " + entry.path;
      const copy = element("span", "project-copy");
      copy.append(element("strong", "", entry.name), element("small", "", entry.displayPath));
      const count = element("span", "project-visits", entry.visits + (entry.visits === 1 ? " visit" : " visits"));
      button.append(icon("folder"), copy, count);
      const remove = element("button", "icon project-remove");
      remove.type = "button";
      remove.dataset.path = entry.path;
      remove.title = "Remove from project history (⌘W)";
      remove.setAttribute("aria-label", "Remove " + entry.name + " from project history");
      remove.setAttribute("aria-keyshortcuts", "Meta+W");
      remove.append(icon("close"));
      row.append(button, remove);
      return row;
    }));
    $("projects-empty").textContent = this.entries.length
      ? "No projects match your search."
      : "Visit a project folder in Minterm to see it here.";
    $("projects-empty").hidden = entries.length > 0;
  }
}
