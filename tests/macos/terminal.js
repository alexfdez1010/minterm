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
const command = 1 << 20, control = 1 << 18, shift = 1 << 17;
const key = (code, text, modifiers = 0, plain = text, phase = '') =>
  window.webkit.messageHandlers.testKey.postMessage({code, text, modifiers, plain, phase});
const controlLetter = (code, letter, phase) =>
  key(code, String.fromCharCode(letter.toUpperCase().charCodeAt(0) & 31), control, letter, phase);
const keyPhase = (phase, code, text) => key(code, text, 0, text, phase);
async function type(text) {
  for (const letter of text) await key(0, letter);
}
async function enter(text) {
  await type(text);
  await key(36, '\r');
}
function contrast(first, second) {
  const luminance = color => {
    const [r, g, b] = color.match(/[\d.]+/g).slice(0, 3).map(value => {
      const channel = Number(value) / 255;
      return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const a = luminance(first), b = luminance(second);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}
function verifyCloseContrast(dialog, button) {
  const resting = getComputedStyle(button).backgroundColor;
  const changed = [];
  // AppKit has no pointing device over its hidden test window. Give :hover
  // a test-only equivalent of equal specificity in the actual CSS cascade.
  try {
    for (const sheet of document.styleSheets) for (const rule of sheet.cssRules) {
      if (!rule.selectorText?.includes(':hover')) continue;
      changed.push([rule, rule.selectorText]);
      rule.selectorText = rule.selectorText.replaceAll(':hover', ':is(:hover, [data-qa-hover])');
    }
    button.setAttribute('data-qa-hover', '');
    const style = getComputedStyle(button), surface = style.backgroundColor;
    if (contrast(style.color, surface) < 4.5 || contrast(surface, getComputedStyle(dialog).backgroundColor) < 3 ||
        contrast(surface, resting) < 3) throw new Error('Close tab loses contrast on hover');
  } finally {
    button.removeAttribute('data-qa-hover');
    for (const [rule, selector] of changed) rule.selectorText = selector;
  }
}
try {
  await expect('First shell and its Git branch', () => tabs().length === 1 &&
    $('.terminal-page.selected')?.dataset.state === 'live' && selected().title.includes('main') && selected().title.includes('/workspace'));
  const zoomTerminal = activeTerminal();
  const firstFamily = stack => stack.split(',')[0].trim().replaceAll('"', '');
  const terminalFamily = 'Minterm Mono';
  for (const weight of ['400', '700']) {
    if (![...document.fonts].some(face =>
      firstFamily(face.family) === terminalFamily && face.weight === weight && face.status === 'loaded')) {
      const faces = [...document.fonts].map(face => [face.family, face.weight, face.status]);
      throw new Error('Terminal font weight ' + weight + ' was not loaded for the first shell: ' + JSON.stringify(faces));
    }
  }
  if (firstFamily(zoomTerminal.options.fontFamily) !== terminalFamily) {
    throw new Error('Terminal does not use the bundled Minterm Mono font');
  }
  const originalFontSize = zoomTerminal.options.fontSize;
  await key(30, '+', command); // Spanish keyboard: dedicated plus key.
  await expect('Command-plus increases text size', () => zoomTerminal.options.fontSize === originalFontSize + 1);
  await key(24, '+', command | shift); // US keyboard: Shift-Equals.
  await expect('Shifted Command-plus increases text size', () => zoomTerminal.options.fontSize === originalFontSize + 2);
  await key(27, '-', command);
  await expect('Command-minus decreases text size', () => zoomTerminal.options.fontSize === originalFontSize + 1);
  await key(29, '0', command);
  await expect('Command-zero restores text size', () => zoomTerminal.options.fontSize === originalFontSize);
  await expect('Terminal directly below the tabs', () => !$('.workspace .context') &&
    $('.workspace').firstElementChild === $('#terminals'));
  await expect('Inspector starts with Agents expanded and Ports collapsed', () =>
    document.querySelectorAll('#inspect-panel details').length === 2 &&
    $('#agent-section').open && !$('#port-section').open);
  const terminal = activeTerminal();
  // Simulate WebKit replacing the last character in xterm's input textarea
  // after a composition keydown, without starting a composition.
  const input = terminal.textarea;
  const dispatchKeydown = (key, keyCode) =>
    input.dispatchEvent(new KeyboardEvent('keydown', { key, keyCode, bubbles: true, cancelable: true }));
  const replaceInput = (text, replaced) => {
    dispatchKeydown('Process', 229);
    input.setRangeText(text, input.value.length - replaced, input.value.length, 'end');
  };
  const emitted = [];
  const subscription = terminal.onData(data => emitted.push(data));
  try {
    for (const letter of 'TRACE') await key(0, letter, shift, letter.toLowerCase());
    await expect('Native keys reached xterm once', () => emitted.join('') === 'TRACE');
    await expect('Recent input remains in the helper textarea', () => input.value === 'TRACE');
    for (const [replacement, replaced] of [['é', 1], ['🙂', 1], ['!', 0], ['!', 0]]) {
      const previous = emitted.length;
      replaceInput(replacement, replaced);
      await expect('New text reached xterm', () => emitted.length > previous);
      const data = emitted.slice(previous).join('');
      if (data !== replacement) throw new Error('Unexpected terminal input: ' + JSON.stringify(data));
    }

    const beforeLetter = emitted.length;
    replaceInput('?', 1);
    dispatchKeydown('r', 82);
    // Synthetic keydown does not perform WebKit's default textarea insertion.
    input.setRangeText('r', input.value.length, input.value.length, 'end');
    await pause(30);
    const letter = emitted.slice(beforeLetter).join('');
    if (letter !== '?r') throw new Error('Input overlapped a normal key: ' + JSON.stringify(letter));

    const beforeClear = emitted.length;
    await controlLetter(8, 'c');
    await expect('Control-C clears the prior shell input', () => emitted.slice(beforeClear).join('') === '\x03');
    const beforeEnter = emitted.length;
    input.value = 'TRACE';
    dispatchKeydown('Process', 229);
    dispatchKeydown('Enter', 13);
    await pause(30);
    const enterData = emitted.slice(beforeEnter).join('');
    if (enterData !== '\r') throw new Error('Input overlapped Enter: ' + JSON.stringify(enterData));
  } finally {
    subscription.dispose();
    await controlLetter(8, 'c');
  }
  const hasPrompt = (folder, branch) => rows(terminal).some(line =>
    line.includes(folder) && line.includes(branch) && line.includes('❯'));
  await expect('Prompt names the working directory and branch', () => hasPrompt('workspace', 'main'));
  await document.fonts.ready;
  await expect('Bundled interface font', () => document.fonts.check('13px Barlow'));
  const first = selected();
  await enter("cd nested; pwd > tested-cwd; printf '<%s>\\n' ready");
  await expect('Shell output and Git branch after cd', () =>
    selected().title.includes('agent/review') && selected().title.includes('/workspace/nested') &&
    rows(terminal).some(line => line.includes('<ready>')));
  await expect('Prompt follows cd into another repository', () => hasPrompt('nested', 'agent/review'));
  await enter('../bin/codex');
  await expect('Manual CLI is ready before its tab goes into the background', () =>
    rows(terminal).some(line => line.includes('Do you want to proceed? [y/n]')));
  for (let count = 2; count <= 9; count++) {
    await key(17, 't', command);
    await expect('One new live tab per Command-T', () => tabs().length === count &&
      selected() === tabs()[count - 1] && $('.terminal-page.selected')?.dataset.state === 'live');
  }
  await expect('New tab inherits the nested directory', () =>
    selected().title.includes('/workspace/nested') && selected().title.includes('agent/review'));
  await key(40, 'k', command);
  await expect('Agent draft accepts keyboard input', () => document.activeElement === $('#command-input'));
  if (firstFamily(getComputedStyle($('#command-input')).fontFamily) !== terminalFamily) {
    throw new Error('Agent draft does not use the terminal font');
  }

  // Hold Git replies during selection. Number shortcuts must focus the requested
  // terminal immediately, including when focus starts in an unsent agent draft.
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
        document.activeElement === (index === 8 ? $('#command-input') : $('.terminal-page.selected textarea.xterm-helper-textarea')));
    }
    await key(53, '\x1b');
    await expect('Escape restores the ninth shell', () => document.activeElement === $('.terminal-page.selected textarea.xterm-helper-textarea'));
  } finally {
    window.minterm.receive = receive;
    for (const context of delayed) receive('context', context);
  }
  await key(18, '&', command); // The unshifted Digit1 symbol on a French keyboard.
  await expect('Command-Digit1 also works with a nonnumeric layout symbol', () =>
    selected() === first && document.activeElement === $('.terminal-page.selected textarea.xterm-helper-textarea'));
  await key(48, '\t', control);
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
  $('#agents .agent-link').click();
  await expect('Agent row opens its terminal', () => selected() === first);
  const ports = $('#port-section'), initial = ports.open;
  ports.querySelector('summary').click();
  await expect('Port section toggles', () => ports.open !== initial);
  const serviceRows = () => [...ports.querySelectorAll('#ports li')];
  const service = name => serviceRows().find(row => row.querySelector('.service-name').textContent === 'fixture-' + name);
  await expect('Development includes databases and mapped app ports', () =>
    service('postgres')?.textContent.includes('127.0.0.1:') && service('postgres').textContent.includes('5432/tcp') &&
    service('postgres').textContent.includes('running') && service('cache')?.textContent.includes('No host port') &&
    service('web')?.textContent.includes('127.0.0.1:9091 → 8080/tcp') && !service('other') && !service('private'));
  $('#all-filter').click();
  const boundEndpoint = '127.0.0.1' + service('postgres').querySelector('.port-number').textContent;
  await expect('All ports includes other published apps and combines overlapping listeners', () =>
    $('#all-filter').getAttribute('aria-pressed') === 'true' && service('postgres') && service('cache') &&
    service('other')?.textContent.includes('local/other:1') && !service('private') &&
    !serviceRows().some(row => row.title.startsWith(boundEndpoint + ' ')));
  $('#dev-filter').click();
  ports.querySelector('summary').click();
  await expect('Port section restores', () => ports.open === initial);
  // Launch from the last idle shell so returning focus is unambiguous.
  await key(28, '8', command);
  await expect('Idle shell selected for launch', () => selected() === tabs()[7]);
  const sourceTab = selected(), beforeLaunch = tabs().length;
  const surface = () => $('.terminal-page.selected');
  $('#start-agent').click();
  await expect('Agent launcher opened', () => $('#agent-launch').open);
  const agentChoice = $('#agent-launch-form').elements.agent;
  await expect('Launcher lists built-in agents and the saved shell command', () =>
    [...agentChoice.options].map(option => option.value).sort().join(',') === 'claude,codex,ocq,opencode,sol-light,sol-ultra');
  const task = $('#agent-prompt');
  task.value = 'Review fixture changes\nDescribe `git status` literally.';
  task.dispatchEvent(new Event('input', {bubbles:true}));
  $('#agent-submit').click();
  await expect('Agent launcher opens a live TUI with the task title', () => tabs().length === beforeLaunch + 1 &&
    surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live' &&
    selected().textContent.includes('Review fixture changes'));
  const launchedTab = selected(), launched = activeTerminal();
  const launchedRow = () => [...document.querySelectorAll('#agents li')]
    .find(row => row.textContent.includes('Review fixture changes'));
  await expect('Inspector shows local activity from the CLI', () =>
    launchedRow()?.textContent.includes('Running a command') &&
    launchedRow()?.querySelector('.agent-status')?.dataset.state === 'running');
  await key(13, 'w', command);
  const confirm = $('#confirm'), close = $('#confirm-close');
  await expect('Command-W on the live agent asks before closing', () =>
    confirm.open && close.textContent === 'Close tab' && document.activeElement === $('#cancel'));
  verifyCloseContrast(confirm, close);
  $('#cancel').click();
  await expect('Cancel keeps the running session selected', () => !confirm.open && selected() === launchedTab);
  await key(28, '8', command);
  await expect('Source shell stays available during the run', () => selected() === sourceTab);
  await key(25, '9', command);
  await expect('Running tab is still available', () => selected() === launchedTab);
  await enter('review complete');
  await expect('The agent launched from the inspector accepts follow-up input', () =>
    rows(launched).some(line => line.includes('Fixture reply: review complete')));
  await enter('/exit');
  await expect('Finished run remains in Agents', () =>
    launchedRow()?.querySelector('.agent-status')?.dataset.state === 'completed');
  await key(13, 'w', command);
  await expect('Closing the run returns to the source shell', () =>
    tabs().length === beforeLaunch && selected() === sourceTab &&
    document.activeElement === $('.terminal-page.selected textarea.xterm-helper-textarea'));
  const sourceTerminal = activeTerminal();
  await enter('cd "$HOME/command-target" && printf "%s\\n" one two three four');
  await expect('Source shell changed into a different repository', () =>
    rows(sourceTerminal).some(line => line.includes('command/fixture') && line.includes('❯')));
  // Leave a shell command partially typed to distinguish its first cell from its
  // cursor. The robot must replace the actual chevron after the previous output.
  await type('echo retained');
  await expect('The shell has the unfinished command', () =>
    rows(sourceTerminal).some(line => line.includes('❯ echo retained')));
  const originalRows = sourceTerminal.rows, originalCols = sourceTerminal.cols;
  const screen = sourceTerminal.element.querySelector('.xterm-screen').getBoundingClientRect();
  const cellHeight = screen.height / originalRows, cellWidth = screen.width / originalCols;
  function promptPosition() {
    const buffer = sourceTerminal.buffer.active;
    const row = buffer.baseY + buffer.cursorY;
    const line = buffer.getLine(row);
    let marker = buffer.cursorX;
    while (marker > 0 && line.getCell(marker)?.getChars() !== '❯') marker--;
    if (line.getCell(marker)?.getChars() !== '❯') throw new Error('Missing prompt marker on the editable line');
    return {left: screen.left + marker * cellWidth, top: screen.top + (row - buffer.viewportY) * cellHeight};
  }
  function editorAtPrompt(position) {
    const marker = $('#command-marker').getBoundingClientRect();
    const input = $('#command-input').getBoundingClientRect();
    return Math.abs(marker.left - position.left) < cellWidth / 2 &&
      Math.abs(marker.top - position.top) < cellHeight / 2 &&
      Math.abs(input.left - (position.left + 2 * cellWidth)) < cellWidth / 2 &&
      Math.abs(input.top - position.top) < cellHeight / 2;
  }
  const firstPrompt = promptPosition();
  await key(40, 'k', command);
  await expect('Command-K focuses a blank input marked only by a robot', () =>
    $('#command-toggle').getAttribute('aria-pressed') === 'true' &&
    $('#command-marker svg') && !$('#command-input').placeholder && document.activeElement === $('#command-input'));
  await expect('Robot replaces the shell chevron and input starts at the command', () => editorAtPrompt(firstPrompt));
  const modeTask = 'touch shell-must-not-run';
  await type(modeTask);
  await key(53, '\x1b');
  await expect('Escape returns keyboard input to the terminal', () =>
    $('#command-form').hidden && document.activeElement === $('.terminal-page.selected textarea.xterm-helper-textarea'));
  await key(36, '\r');
  await expect('Returning to the shell preserves its unfinished command', () =>
    rows(sourceTerminal).some(line => line === 'retained') &&
    sourceTerminal.buffer.active.getLine(sourceTerminal.buffer.active.baseY + sourceTerminal.buffer.active.cursorY)?.translateToString(true).includes('❯'));
  const secondPrompt = promptPosition();
  if (secondPrompt.top <= firstPrompt.top + cellHeight) throw new Error('Expected a different input row');
  await key(26, '7', command);
  await expect('Seventh tab selected before opening its draft', () => selected() === tabs()[6]);
  $('#command-toggle').click();
  await expect('A different tab has its own command draft', () =>
    !$('#command-form').hidden && $('#command-input').value === '');
  await key(53, '\x1b');
  await key(28, '8', command);
  await expect('Source tab selected before restoring its draft', () => selected() === sourceTab);
  $('#command-toggle').click();
  await expect('The source tab restores its command draft', () => $('#command-input').value === modeTask);
  await expect('The restored draft follows the new cursor position', () => editorAtPrompt(secondPrompt));
  await key(36, '\r', shift);
  await type('Review the current directory');
  await expect('Shift-Enter creates a second task line', () =>
    $('#command-input').value === modeTask + '\nReview the current directory');
  await expect('Multiline input preserves the terminal grid', () =>
    sourceTerminal.rows === originalRows && sourceTerminal.cols === originalCols &&
    Math.abs(sourceTerminal.element.querySelector('.xterm-screen').getBoundingClientRect().height - screen.height) < 1);
  await key(36, '\r');
  await expect('A companion PTY opens within the selected tab', () => tabs().length === beforeLaunch &&
    selected() === sourceTab && surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live' &&
    $('#command-form').hidden && $('#command-toggle').getAttribute('aria-pressed') === 'true');
  const commandTerminal = activeTerminal();
  await expect('Interactive CLI has a real TTY', () =>
    rows(commandTerminal).some(line => line.includes('Interactive fixture ready')));
  await enter('First message');
  await expect('Interactive CLI reads the next message', () =>
    rows(commandTerminal).some(line => line.includes('Fixture reply: First message')));
  await key(40, 'k', command);
  await expect('Shell input is restored from agent mode', () =>
    selected() === sourceTab && tabs().length === beforeLaunch && surface()?.dataset.kind === 'shell' &&
    document.activeElement === $('.terminal-page.selected textarea.xterm-helper-textarea'));
  await enter('echo restored | tee shell-restored');
  await expect('The shell runs the command after switching back', () => rows(sourceTerminal).includes('restored'));
  $('#command-toggle').click();
  await expect('Switching back restores the existing agent terminal', () =>
    surface()?.dataset.kind === 'agent' && activeTerminal() === commandTerminal);
  await enter('Second message');
  await expect('The restored agent still accepts input', () =>
    rows(commandTerminal).some(line => line.includes('Fixture reply: Second message')));
  await key(40, 'k', command);
  await key(26, '7', command);
  await window.webkit.messageHandlers.testKey.postMessage({menuItem:'New Agent Tab'});
  await expect('The native menu opens a new agent tab', () => tabs().length === beforeLaunch + 1 &&
    surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live');
  const newAgent = activeTerminal();
  await expect('An empty task opens the interactive CLI', () =>
    rows(newAgent).some(line => line.includes('Interactive fixture ready')));
  await enter('/exit');
  await expect('The direct agent tab observes CLI exit', () => surface().dataset.state === 'ended');
  await key(13, 'w', command);
  await expect('Only the direct agent tab closes', () => tabs().length === beforeLaunch);
  await key(26, '7', command);
  await expect('Seventh tab selected for the bare alias', () => selected() === tabs()[6]);
  await key(40, 'k', command);
  await expect('Seventh shell offers an empty agent prompt', () =>
    document.activeElement === $('#command-input') && $('#command-input').value === '');
  await key(36, '\r');
  await expect('Return on an empty robot prompt starts the default agent in place', () =>
    tabs().length === beforeLaunch && surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live');
  const blankAgent = activeTerminal();
  await expect('Default agent starts without a prompt argument', () =>
    rows(blankAgent).includes('Started codex without a task'));
  await enter('/exit');
  await expect('Default agent exits', () => surface().dataset.state === 'ended');
  await key(40, 'k', command);
  await expect('Source shell returns after default agent exit', () => surface().dataset.kind === 'shell');
  await key(40, 'k', command);
  await expect('Robot prompt is ready for the bare alias', () => document.activeElement === $('#command-input'));
  await enter('ocq');
  await expect('Typing the bare profile name selects it over the default Codex', () =>
    surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live' && selected().textContent.includes('ocq'));
  const aliasTerminal = activeTerminal();
  await expect('Shell alias supplied its model to the interactive CLI', () =>
    rows(aliasTerminal).includes('Model: fixture/qwen'));
  await enter('/exit');
  await expect('Bare alias ends its interactive session', () => surface().dataset.state === 'ended');
  await key(28, '8', command);
  await expect('Source shell selected for the next agents', () => selected() === sourceTab && surface().dataset.kind === 'shell');
  await key(17, 't', command);
  await expect('New terminal tab inherits the command target directory', () =>
    tabs().length === beforeLaunch + 1 && surface()?.dataset.state === 'live' && selected().title.includes('/command-target'));
  await key(40, 'k', command);
  await expect('Agent prompt is ready for a named task', () => document.activeElement === $('#command-input'));
  await type('ocq --help review');
  await key(36, '\r', shift);
  await enter('Keep $(touch alias-command-ran) literal.');
  await expect('Named task starts an agent in the same tab', () =>
    surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live' && tabs().length === beforeLaunch + 1);
  const namedTask = activeTerminal();
  await expect('Named task uses the configured model', () => rows(namedTask).includes('Model: fixture/qwen'));
  await enter('continue');
  await expect('Custom command accepts interactive follow-up input', () => rows(namedTask).includes('Fixture reply: continue'));
  await enter('/exit');
  await expect('Custom command exits', () => surface().dataset.state === 'ended');
  for (const [letter, code] of [['l', 37], ['u', 32]]) {
    await key(40, 'k', command);
    await expect('Shell is ready after agent exit', () => surface()?.dataset.kind === 'shell');
    await key(40, 'k', command);
    await expect('Agent prompt is ready for a launch shortcut', () => document.activeElement === $('#command-input'));
    if (letter === 'l') {
      await type('Test the change');
      await key(36, '\r', shift);
      await type('Review this diff.');
    }
    await controlLetter(code, letter, letter === 'l' ? 'down' : '');
    await expect('Ctrl+' + letter + ' launches a live terminal', () =>
      surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live');
    const cli = activeTerminal();
    await expect('The shortcut chose the Sol model', () => rows(cli).includes('Model: gpt-6.2-sol'));
    if (letter === 'l') {
      // Release Control first and keep holding L as focus moves into the new PTY.
      await keyPhase('repeat', code, letter);
      await keyPhase('up', code, letter);
    }
    if (letter === 'u') {
      // The PTY owns Ctrl+U after launch. Its line discipline erases this input.
      await type('discard');
      await controlLetter(32, 'u');
    }
    await enter('follow up');
    await expect('Agent input is free of launch keys', () => rows(cli).includes('Fixture reply: follow up'));
    await enter('/exit');
    await expect('The shortcut session exits', () => surface().dataset.state === 'ended');
  }
  await key(13, 'w', command);
  await expect('Closing the custom command returns to its source', () => selected() === sourceTab);
  for (const [name, prompt] of [
    ['claude', ''],
    ['opencode', ' \n  '],
    ['claude', 'Read the fixture'],
    ['opencode', 'Review the fixture\nKeep `pwd` literal.'],
  ]) {
    $('#start-agent').click();
    await expect(name + ' launcher opened', () => $('#agent-launch').open);
    agentChoice.value = name;
    agentChoice.dispatchEvent(new Event('change', {bubbles: true}));
    if (prompt) {
      task.value = prompt;
      task.dispatchEvent(new Event('input', {bubbles:true}));
    }
    const title = prompt.trim() ? prompt.split('\n')[0] : agentChoice.selectedOptions[0].label;
    await expect(name + ' can start with the current task', () => !$('#agent-submit').disabled);
    $('#agent-submit').click();
    await expect(name + ' launches in a live PTY', () => tabs().length === beforeLaunch + 1 &&
      selected().textContent.includes(title) && selected().title.includes('/command-target') &&
      surface().dataset.kind === 'agent' && surface().dataset.state === 'live');
    const cli = activeTerminal();
    if (!prompt.trim()) {
      await expect(name + ' starts without a prompt argument', () => rows(cli).includes('Started ' + name + ' without a task'));
    }
    await enter('follow up');
    await expect(name + ' handles subsequent input', () => rows(cli).some(line => line.includes('Fixture reply: follow up')));
    await enter('/exit');
    await expect(name + ' exits', () => surface().dataset.state === 'ended');
    await key(13, 'w', command);
    await expect(name + ' closes independently', () => selected() === sourceTab && tabs().length === beforeLaunch);
  }
  await verifyPreferences();
  await key(17, 't', command);
  await expect('Terminal is ready to edit its external profile file', () =>
    tabs().length === beforeLaunch + 1 && surface()?.dataset.kind === 'shell' && surface().dataset.state === 'live');
  const external = activeTerminal();
  await enter('mkdir -p "$HOME/external" && cd "$HOME/external" && cat "$HOME/extra-profile.toml" >> "$XDG_CONFIG_HOME/minterm/agents.toml" && echo updated');
  await expect('Shell finished changing the TOML on disk', () => rows(external).includes('updated'));
  await key(40, 'k', command);
  await expect('Agent prompt opens after editing the configuration', () => document.activeElement === $('#command-input'));
  await type('review external edit');
  await controlLetter(15, 'r');
  await expect('Newly written profile starts without reopening Settings or the window', () =>
    surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live');
  const fresh = activeTerminal();
  await expect('New profile supplies its own model', () => rows(fresh).includes('Model: fixture/external'));
  await enter('/exit');
  await expect('External profile exits', () => surface().dataset.state === 'ended');
  for (const heldReturn of [true, false]) {
    await key(40, 'k', command);
    await expect('Shell returns after external agent', () => surface()?.dataset.kind === 'shell');
    await key(40, 'k', command);
    await expect('Prompt is ready for a legacy shortcut', () => document.activeElement === $('#command-input'));
    if (heldReturn) {
      await keyPhase('down', 36, '\r');
      await key(5, 'g');
      await keyPhase('up', 36, '\r');
    } else {
      await controlLetter(5, 'g');
    }
    await expect('Legacy shortcut launches its agent', () =>
      surface()?.dataset.kind === 'agent' && surface().dataset.state === 'live');
    const cli = activeTerminal();
    await expect('Legacy shortcut opens Claude with no prompt', () => rows(cli).includes('Started claude without a task'));
    await enter('/exit');
    await expect('Legacy shortcut agent exits', () => surface().dataset.state === 'ended');
  }
  return 'passed';
} catch (err) {
  return String(err) + '\n' + JSON.stringify({
    tab: selected()?.textContent, focus: document.activeElement.id || document.activeElement.className,
    surface: $('.terminal-page.selected')?.dataset, draft: $('#command-input').value,
    reason: $('#exit-note')?.textContent,
  }) + '\n' + err.stack;
}
