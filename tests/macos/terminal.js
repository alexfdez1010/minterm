// Executed inside the app's own WKWebView. Read visible UI state and send native keys.
const $ = (query) => document.querySelector(query);
const tabs = () => [...document.querySelectorAll('[role="tab"]')];
const selected = () => $('[role="tab"][aria-selected="true"]');
// WebKit defers animation frames and accessibility updates in a hidden window.
// Capture xterm's public parsed cell buffer when the real tab takes focus.
function activeTerminal() {
  const focus = Terminal.prototype.focus;
  let active;
  try {
    Terminal.prototype.focus = function () { active = this; return focus.call(this); };
    selected().click();
  } finally { Terminal.prototype.focus = focus; }
  return active;
}
function rows(terminal) {
  const buffer = terminal.buffer.active;
  return Array.from({length: terminal.rows}, (_, i) =>
    buffer.getLine(buffer.viewportY + i)?.translateToString(true) || '');
}
const pause = (ms) => new Promise(resolve => setTimeout(resolve, ms));
async function expect(description, condition) {
  const deadline = Date.now() + 8000;
  while (!condition()) {
    if (Date.now() > deadline) throw new Error(description);
    await pause(50);
  }
}
const command = 1 << 20, shift = 1 << 17;
const key = (code, text, modifiers = 0, plain = text) =>
  window.webkit.messageHandlers.testKey.postMessage({code, text, modifiers, plain});
async function enter(text) {
  for (const letter of text) await key(0, letter);
  await key(36, '\r');
}
try {
  await expect('First shell and its Git branch', () => tabs().length === 1 &&
    $('.terminal-page.selected')?.dataset.state === 'live' && selected().title.includes('main') && selected().title.includes('/workspace'));
  await expect('Terminal directly below the tabs', () => !$('.workspace .context') &&
    $('.workspace').firstElementChild === $('#terminals'));
  await expect('Only Agents initially expanded', () => $('#agent-section').open &&
    !$('#port-section').open && !$('#db-section').open);
  const terminal = activeTerminal();
  const hasPrompt = (folder, branch) => rows(terminal).some(line =>
    line.includes(folder) && line.includes(branch) && line.includes('❯'));
  await expect('Prompt names the working directory and branch', () => hasPrompt('workspace', 'main'));
  await document.fonts.ready;
  await expect('Bundled terminal and interface fonts', () =>
    document.fonts.check('13px "Martian Mono"') && document.fonts.check('13px Barlow'));
  const first = selected();
  await enter("cd nested; pwd > tested-cwd; printf '<%s>\\n' ready");
  await expect('Shell output and Git branch after cd', () =>
    selected().title.includes('agent/review') && selected().title.includes('/workspace/nested') &&
    rows(terminal).some(line => line.includes('<ready>')));
  await expect('Prompt follows cd into another repository', () => hasPrompt('nested', 'agent/review'));
  await enter('../bin/codex');
  for (let count = 2; count <= 9; count++) {
    await key(17, 't', command);
    await expect('One new live tab per Command-T', () => tabs().length === count &&
      selected() === tabs()[count - 1] && $('.terminal-page.selected')?.dataset.state === 'live');
  }
  await expect('New tab inherits the nested directory', () =>
    selected().title.includes('/workspace/nested') && selected().title.includes('agent/review'));
  await key(38, 'j', command);
  await expect('Assistant accepts keyboard input', () => document.activeElement === $('#question'));

  // Hold Git replies during selection. Number shortcuts must focus the requested
  // terminal immediately, including when the current focus is in Assistant.
  const digits = [18, 19, 20, 21, 23, 22, 26, 28, 25]; // macOS physical key codes for 1 through 9
  const targets = tabs(), receive = window.minterm.receive;
  const delayed = [];
  window.minterm.receive = (name, data) => {
    if (name === 'context') delayed.push(data);
    else receive(name, data);
  };
  try {
    for (const [index, code] of digits.entries()) {
      await key(code, String(index + 1), command);
      await expect('Command-' + (index + 1) + ' selects and focuses its terminal', () =>
        selected() === targets[index] &&
        document.activeElement === $('.terminal-page.selected textarea.xterm-helper-textarea'));
    }
  } finally {
    window.minterm.receive = receive;
    for (const context of delayed) receive('context', context);
  }
  await key(18, '&', command); // The unshifted Digit1 symbol on a French keyboard.
  await expect('Command-Digit1 also works with a nonnumeric layout symbol', () =>
    selected() === first && document.activeElement === $('.terminal-page.selected textarea.xterm-helper-textarea'));
  await key(48, '\t', 1 << 18);
  await expect('Control-Tab changes tabs', () => selected() === tabs()[1]);
  await key(30, '}', command | shift);
  await expect('Command-Shift-] selects the next tab', () => selected() === tabs()[2]);
  await key(33, '{', command | shift);
  await expect('Command-Shift-[ selects the previous tab', () => selected() === tabs()[1]);
  await key(13, 'w', command);
  await expect('Command-W closes one idle tab', () => tabs().length === 8 && selected() !== first);
  if ($('#agent-section').open) $('#agent-section summary').click();
  await key(0, 'A', command | shift);
  await expect('Command-Shift-A expands the agents section', () => $('#agent-section').open);
  await expect('Agent in the inactive tab', () => $('#agent-section').open &&
    $('#agents button strong')?.textContent === 'Codex');
  await expect('Manual agent is asking for input', () =>
    $('#agents .agent-status[data-state="inputRequested"]') !== null);
  $('#agents .agent-summarize').click();
  await expect('Manual summary previews the terminal excerpt', () => $('#agent-summary').open &&
    $('#agent-excerpt').value.includes('Do you want to proceed?'));
  $('#agent-excerpt').value = 'The fixture is waiting for an answer.';
  $('#excerpt-submit').click();
  await expect('Manual summary explains missing key', () =>
    $('#agents .agent-summary-text.error')?.textContent.includes('OPEN_API_KEY'));
  $('#agents .agent-link').click();
  await expect('Agent row opens its terminal', () => selected() === first);
  const ports = $('#port-section'), initial = ports.open;
  ports.querySelector('summary').click();
  await expect('Port section toggles', () => ports.open !== initial);
  ports.querySelector('summary').click();
  await expect('Port section restores', () => ports.open === initial);
  // Launch from the last idle shell so returning focus is unambiguous.
  await key(28, '8', command);
  await expect('Idle shell selected for launch', () => selected() === tabs()[7]);
  const sourceTab = selected(), beforeLaunch = tabs().length;
  const summaries = new Map(), receiveWithSummary = window.minterm.receive;
  window.minterm.receive = (name, data) => {
    if (name === 'summary') summaries.set(data.id, data);
    receiveWithSummary(name, data);
  };
  $('#start-agent').click();
  await expect('Agent launcher opened', () => $('#agent-launch').open);
  await expect('Launch has no summary opt-in or provider selector', () =>
    !$('#agent-launch input[type="checkbox"]') && !$('#agent-launch select'));
  const task = $('#agent-prompt');
  task.value = 'Review fixture changes\nDescribe `git status` literally.';
  task.dispatchEvent(new Event('input', {bubbles:true}));
  $('#agent-submit').click();
  await expect('Managed tab has a task title', () => tabs().length === beforeLaunch + 1 &&
    selected().textContent.includes('Review fixture changes'));
  const managedTab = selected(), managed = activeTerminal();
  const managedRow = () => [...document.querySelectorAll('#agents li')]
    .find(row => row.textContent.includes('Review fixture changes'));
  const codexId = Number(managedTab.id.slice('tab-'.length));
  await expect('Codex silently skips summaries without its own key', () =>
    summaries.get(codexId)?.skipped === true && managedRow()?.querySelector('.agent-summary-text')?.hidden &&
    managedRow()?.querySelector('.agent-detail')?.textContent === 'Running a command' &&
    managedRow()?.querySelector('.agent-status')?.dataset.state === 'running');
  await key(28, '8', command);
  await expect('Source shell stays available during the run', () => selected() === sourceTab);
  await enter('echo done > "$HOME/finish-tool"');
  await key(25, '9', command);
  await expect('Running tab is still available', () => selected() === managedTab);
  await expect('Structured agent reply reaches the terminal', () =>
    rows(managed).some(line => line.includes('Fixture review complete.')));
  await expect('Finished run remains in Agents', () =>
    managedRow()?.querySelector('.agent-status')?.dataset.state === 'completed');
  await key(13, 'w', command);
  await expect('Closing the run returns to the source shell', () =>
    tabs().length === beforeLaunch && selected() === sourceTab &&
    document.activeElement === $('.terminal-page.selected textarea.xterm-helper-textarea'));
  await key(38, 'j', command);
  await expect('Assistant opens', () => !$('#assist-panel').hidden);
  $('#question').value = 'What does pwd print?';
  $('#question').dispatchEvent(new Event('input', {bubbles: true}));
  $('#ask').click();
  await expect('Missing key is actionable', () => !$('#reply').hidden && $('#answer').textContent.includes('OPEN_API_KEY'));
  await key(34, 'i', command);
  await expect('Inspector available for Claude', () => !$('#inspect-panel').hidden);
  $('#start-agent').click();
  await expect('Claude launcher opened', () => $('#agent-launch').open);
  $('#agent-launch input[value="claude"]').click();
  task.value = 'Read routing fixture';
  task.dispatchEvent(new Event('input', {bubbles:true}));
  $('#agent-submit').click();
  await expect('Claude task opens', () => selected().textContent.includes('Read routing fixture'));
  const claudeId = Number(selected().id.slice('tab-'.length));
  // Only Anthropic is configured. The local proxy records its host and rejects
  // the test request. Rust checks that no other connection was attempted.
  await expect('Claude automatically attempts a summary with its own key', () =>
    Boolean(summaries.get(claudeId)?.error) && !summaries.get(claudeId).skipped);
  window.minterm.receive = receiveWithSummary;
  return 'passed';
} catch (err) {
  return String(err) + '\n' + err.stack;
}
