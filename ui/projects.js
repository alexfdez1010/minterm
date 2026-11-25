import { $, element, icon, send } from "./shared.js";

export class Projects {
  constructor(openDirectory, restoreFocus) {
    this.dialog = $("projects-dialog");
    this.search = $("project-search");
    this.list = $("project-list");
    this.entries = [];
    this.loaded = false;
    $("projects-toggle").onclick = () => this.open();
    $("projects-close").onclick = () => this.dialog.close();
    this.dialog.addEventListener("close", restoreFocus);
    this.search.addEventListener("input", () => this.render());
    this.list.addEventListener("click", (event) => {
      const button = event.target.closest("button[data-path]");
      if (!button) return;
      this.dialog.close();
      openDirectory(button.dataset.path);
    });
    this.dialog.addEventListener("keydown", (event) => {
      const buttons = [...this.list.querySelectorAll("button")];
      if (event.key === "Escape") {
        event.preventDefault();
        this.dialog.close();
      } else if (event.key === "Enter" && event.target === this.search && buttons.length) {
        event.preventDefault();
        buttons[0].click();
      } else if (["ArrowDown", "ArrowUp"].includes(event.key)) {
        event.preventDefault();
        const index = buttons.indexOf(document.activeElement);
        if (event.key === "ArrowUp" && index <= 0) this.search.focus();
        else buttons[Math.min(buttons.length - 1, index + (event.key === "ArrowDown" ? 1 : -1))]?.focus();
      }
    });
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
    this.entries = entries;
    this.loaded = true;
    $("projects-error").textContent = error || "";
    $("projects-error").hidden = !error;
    this.render();
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
      button.title = "Open a new terminal in " + entry.path;
      const copy = element("span", "project-copy");
      copy.append(element("strong", "", entry.name), element("small", "", entry.displayPath));
      const count = element("span", "project-visits", entry.visits + (entry.visits === 1 ? " visit" : " visits"));
      button.append(icon("folder"), copy, count);
      row.append(button);
      return row;
    }));
    $("projects-empty").textContent = this.entries.length
      ? "No projects match your search."
      : "Visit a project folder in Minterm to see it here.";
    $("projects-empty").hidden = entries.length > 0;
  }
}
