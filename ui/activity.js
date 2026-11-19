import { send } from "./shared.js";

const SUMMARY_INTERVAL = 30000;
const MAX_SUMMARIES = 4;

// Read the live cursor area even when someone has scrolled up through history.
export function terminalExcerpt(term, count = 14) {
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
    this.summaries = new Map();
    this.answered = new Map();
    this.waiting = new Map();
    this.revision = 0;
  }

  scan(agents) {
    for (const old of this.manual) {
      if (
        old.foreground &&
        !agents.some((a) => a.foreground && a.id === old.id && a.pid === old.pid)
      ) {
        this.summaries.delete(old.id);
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
    }
  }

  start(id, { agent, prompt }) {
    this.jobs.set(id, {
      id,
      name: agent === "claude" ? "Claude Code" : "Codex",
      prompt,
      title: prompt.trim().split(/\s+/).join(" ").slice(0, 80),
      managed: true,
      foreground: true,
      state: "running",
      detail: "Starting",
      requests: 0,
      nextRequest: 0,
      steps: [],
    });
    this.publish();
  }

  update({ id, state, detail }) {
    const job = this.jobs.get(id);
    if (!job || job.state === "stopped") return;
    job.state = state === "blocked" ? "needsAttention" : state;
    job.detail = detail;
    if (state === "running" && detail && job.steps.at(-1) !== detail) {
      job.steps.push(detail);
      if (job.steps.length > 4) job.steps.shift();
    }
    this.scheduleSummary(job);
    this.publish();
  }

  scheduleSummary(job) {
    if (
      job.timer ||
      !job.steps.length ||
      job.requestedDetail === job.detail ||
      job.requests >= MAX_SUMMARIES ||
      this.summaries.get(job.id)?.pending
    ) {
      return;
    }
    job.timer = setTimeout(
      () => {
        job.timer = undefined;
        if (!this.jobs.has(job.id) || job.requestedDetail === job.detail) return;
        job.nextRequest = Date.now() + SUMMARY_INTERVAL;
        job.requestedDetail = job.detail;
        this.summarize({
          id: job.id,
          excerpt: JSON.stringify({ task: job.prompt, recentSteps: job.steps, state: job.state }),
        });
      },
      Math.max(700, job.nextRequest - Date.now()),
    );
  }

  summarize({ id, provider, excerpt }) {
    const previous = this.summaries.get(id);
    if (previous?.pending || !excerpt.trim()) return;
    const revision = ++this.revision;
    this.summaries.set(id, { revision, pending: true, text: previous?.text });
    // Managed jobs omit provider. The native host resolves the CLI's matching
    // provider and checks its key before any network request.
    send("summarize", { id, provider, excerpt, revision });
    this.publish();
  }

  receiveSummary({ id, revision, text, error, skipped }) {
    const previous = this.summaries.get(id);
    if (!previous?.pending || previous.revision !== revision) return;
    this.summaries.set(id, {
      revision,
      text: text || previous.text,
      error: skipped ? undefined : error,
      pending: false,
    });
    const job = this.jobs.get(id);
    if (job) {
      if (!skipped) job.requests++;
      this.scheduleSummary(job);
    }
    this.publish();
  }

  close(id) {
    clearTimeout(this.jobs.get(id)?.timer);
    this.jobs.delete(id);
    this.summaries.delete(id);
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
    const rows = [...agents, ...this.jobs.values()]
      .sort((a, b) => a.id - b.id)
      .map((agent) => {
        const result = agent.foreground ? this.summaries.get(agent.id) : undefined;
        return {
          id: agent.id,
          pid: agent.pid,
          name: agent.name,
          foreground: agent.foreground,
          managed: agent.managed,
          state: agent.state,
          detail: agent.detail,
          title: agent.title,
          summary: result?.text,
          summaryError: result?.error,
          summaryPending: !agent.managed && result?.pending,
        };
      });
    this.render(rows);
  }
}
