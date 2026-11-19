// Executed inside the app's own WKWebView. Read visible UI state and send native keys.
const $ = (query) => document.querySelector(query);
const tabs = () => [...document.querySelectorAll('[role="tab"]')];
const selected = () => $('[role="tab"][aria-selected="true"]');
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
    $('.terminal-page.selected')?.dataset.state === 'live' && $('#branch bdi').textContent === 'main');
  await expect('Inspector defaults to running agents', () => $('#agent-section').open && !$('#port-section').open && !$('#db-section').open);
  await document.fonts.ready;
  await expect('Bundled fonts loaded', () => document.fonts.check('13px "Martian Mono"') && document.fonts.check('13px Barlow'));
  const first = selected();
  await enter("cd nested; pwd > tested-cwd; printf '<%s>\\n' ready; ../bin/codex");
  await expect('Shell output and Git branch after cd', () =>
    $('#branch bdi').textContent === 'agent/review' &&
    $('.terminal-page.selected .xterm-accessibility-tree').textContent.includes('<ready>'));
  for (const count of [2, 3]) {
    await key(17, 't', command);
    await expect('One new live tab per Command-T', () => tabs().length === count &&
      selected() === tabs()[count - 1] && $('.terminal-page.selected')?.dataset.state === 'live');
  }
  await expect('New tab inherits the nested directory', () => $('#cwd').title.endsWith('/workspace/nested'));
  await key(18, '1', command);
  await expect('Command-1 returns to the first tab', () => selected() === first);
  await key(48, '\t', 1 << 18);
  await expect('Control-Tab changes tabs', () => selected() === tabs()[1]);
  await key(30, '}', command | shift);
  await expect('Command-Shift-] selects the next tab', () => selected() === tabs()[2]);
  await key(33, '{', command | shift);
  await expect('Command-Shift-[ selects the previous tab', () => selected() === tabs()[1]);
  await key(13, 'w', command);
  await expect('Command-W closes one idle tab', () => tabs().length === 2 && selected() !== first);
  await key(0, 'A', command | shift);
  await expect('Agent in the inactive tab', () => $('#agent-section').open &&
    $('#agents button strong')?.textContent === 'Codex');
  $('#agents button').click();
  await expect('Agent row opens its terminal', () => selected() === first);
  const ports = $('#port-section'), initial = ports.open;
  ports.querySelector('summary').click();
  await expect('Port section toggles', () => ports.open !== initial);
  ports.querySelector('summary').click();
  await expect('Port section restores', () => ports.open === initial);
  await key(38, 'j', command);
  await expect('Assistant opens', () => !$('#assist-panel').hidden);
  $('#question').value = 'What does pwd print?';
  $('#question').dispatchEvent(new Event('input', {bubbles: true}));
  $('#ask').click();
  await expect('Missing key is actionable', () => !$('#reply').hidden && $('#answer').textContent.includes('OPEN_API_KEY'));
  return 'passed';
} catch (err) {
  return String(err) + '\n' + err.stack;
}
