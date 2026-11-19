import { agentLabel } from "./shared.js";

const ACTIVITY_SAMPLE_MS = 350;

const activityLabels = [
  [/^(?:running (?:a command|tests)|bash\()/i, "Running a command"],
  [/^(?:reading files?|read\()/i, "Reading files"],
  [/^(?:editing files?|edit\(|write\()/i, "Editing files"],
  [/^(?:searching|grep\(|glob\()/i, "Searching"],
  [/^(?:thinking|reasoning|working)(?:\b|…)/i, "Working"],
];

function activityLabel(excerpt) {
  for (const line of excerpt.split("\n").reverse()) {
    const text = line.trim().replace(/^[•●⏺✦\s]+/u, "");
    for (const [pattern, label] of activityLabels) {
      if (pattern.test(text)) return label;
    }
  }
  return "";
}

// Read the live cursor area even when someone has scrolled up through history.
function terminalExcerpt(term, count = 14) {
  if (!term) return "";
  const buffer = term.buffer.active;
  const end = buffer.baseY + buffer.cursorY;
  const lines = [];
  for (let i = Math.max(buffer.baseY, end - count + 1); i <= end; i++) {
    const line = buffer.getLine(i)?.translateToString(true);
    if (line !== undefined) lines.push(line);
  }
  return lines.join("\n").trim().slice(-4000);
}

export function asksForInput(excerpt) {
  const lines = excerpt
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(-8);
  const last = lines.at(-1) || "";
  if (
    /\[(?:y\/n|yes\/no)\]|\((?:y\/n|yes\/no)\)|press (?:enter|return|any key) to (?:continue|confirm)/i.test(
      last,
    )
  )
    return true;
  const choices =
    /(?:allow once|yes, (?:proceed|allow)|esc to (?:cancel|deny)|enter to (?:confirm|select))/i;
  return (
    lines.some((line) =>
      /(?:would you like to|do you want to|allow .+\?|requires (?:approval|permission))/i.test(
        line,
      ),
    ) && lines.slice(-4).some((line) => choices.test(line))
  );
}

export class Activity {
  constructor(terminal, render) {
    this.terminal = terminal;
    this.render = render;
    this.manual = [];
    this.jobs = new Map();
    this.answered = new Map();
    this.waiting = new Map();
  }

  scan(agents) {
    agents = agents.filter((agent) => !this.jobs.has(agent.id));
    for (const old of this.manual) {
      if (
        old.foreground &&
        !agents.some((a) => a.foreground && a.id === old.id && a.pid === old.pid)
      ) {
        this.answered.delete(old.id);
        this.waiting.delete(old.id);
      }
    }
    this.manual = agents;
    this.publish();
  }

  preview(id) {
    return terminalExcerpt(this.terminal(id));
  }

  input(id) {
    // Ignore the old prompt until the program has actually changed the screen.
    if (this.waiting.has(id)) {
      this.answered.set(id, this.waiting.get(id));
      this.waiting.delete(id);
      if (this.jobs.has(id)) this.update({ id, state: "running", detail: "Input sent" });
    }
  }

  start(id, { agent, prompt }) {
    this.jobs.set(id, {
      id,
      name: agentLabel(agent),
      title: prompt.trim().split(/\s+/).join(" ").slice(0, 80),
      managed: true,
      foreground: true,
      state: "running",
      detail: "Starting",
    });
    this.publish();
  }

  observe(id) {
    const job = this.jobs.get(id);
    if (!job || job.finished || job.sampleTimer) return;
    job.sampleTimer = setTimeout(() => {
      job.sampleTimer = undefined;
      this.sample(job);
    }, ACTIVITY_SAMPLE_MS);
  }

  sample(job) {
    const excerpt = this.preview(job.id);
    if (!excerpt) return;
    const waiting = excerpt !== this.answered.get(job.id) && asksForInput(excerpt);
    if (waiting) this.waiting.set(job.id, excerpt);
    else this.waiting.delete(job.id);
    const label = waiting ? "Waiting for input" : activityLabel(excerpt);
    const state = waiting ? "inputRequested" : "running";
    const detail = label || (job.title ? "Terminal output" : "Session open");
    if (job.state !== state || job.detail !== detail) this.update({ id: job.id, state, detail });
  }

  finish(id, success, reason) {
    const job = this.jobs.get(id);
    if (!job || job.finished) return;
    clearTimeout(job.sampleTimer);
    this.sample(job);
    this.waiting.delete(id);
    job.finished = true;
    this.update({
      id,
      state: success ? "completed" : "failed",
      detail: success ? "Session ended" : reason || "Agent exited with an error",
    });
  }

  update({ id, state, detail }) {
    const job = this.jobs.get(id);
    if (!job || job.state === "stopped") return;
    job.state = state === "blocked" ? "needsAttention" : state;
    job.detail = detail;
    this.publish();
  }

  close(id) {
    clearTimeout(this.jobs.get(id)?.sampleTimer);
    this.jobs.delete(id);
    this.answered.delete(id);
    this.waiting.delete(id);
    this.manual = this.manual.filter((agent) => agent.id !== id);
    this.publish();
  }

  publish() {
    const agents = this.manual.map((agent) => {
      const excerpt = agent.foreground ? this.preview(agent.id) : "";
      const waiting = !!excerpt && excerpt !== this.answered.get(agent.id) && asksForInput(excerpt);
      if (waiting) this.waiting.set(agent.id, excerpt);
      else if (agent.foreground) this.waiting.delete(agent.id);
      return { ...agent, state: waiting ? "inputRequested" : "running" };
    });
    const rows = [...agents, ...this.jobs.values()].sort((a, b) => a.id - b.id);
    this.render(rows);
  }
}
