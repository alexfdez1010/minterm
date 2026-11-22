// Exercise native Settings and use the selected command in the terminal window.
async function verifyPreferences() {
  const settings = (action, args = {}) => window.webkit.messageHandlers.testKey
    .postMessage({preferences: {action, ...args}});
  function matches(actual, expected) {
    if (expected instanceof RegExp) return typeof actual === 'string' && expected.test(actual);
    if (Array.isArray(expected)) return Array.isArray(actual) && actual.length === expected.length &&
      expected.every((value, i) => matches(actual[i], value));
    if (expected && typeof expected === 'object') return actual &&
      Object.entries(expected).every(([key, value]) => matches(actual[key], value));
    return actual === expected;
  }
  const state = async (expected) => {
    const deadline = Date.now() + 5000;
    let actual;
    do {
      actual = await settings('state');
      if (matches(actual, expected)) return;
      await pause(40);
    } while (Date.now() < deadline);
    throw new Error('Settings ' + JSON.stringify(actual) + ', expected ' + JSON.stringify(expected));
  };
  const open = async () => {
    if (!await key(43, ',', command)) throw new Error('Settings menu shortcut was unavailable');
  };
  const recordKey = (code, letter, modifiers = 0, options = {}) => settings('key', {
    code, letter, modifiers, ...options,
    text: modifiers === control ? String.fromCharCode(letter.toUpperCase().charCodeAt(0) & 31) : letter,
  });
  const defaults = {mode: 'Terminal', shortcut: '⌘K', splitShortcut: '⌘L'};
  const option = 1 << 19;
  await settings('prepare');
  await open();
  const presets = [
    {name: 'codex', title: 'Codex', agent: '', command: 'codex'},
    {name: 'claude', title: 'Claude Code', agent: '', command: 'claude'},
    {name: 'opencode', title: 'OpenCode', agent: '', command: 'opencode', shortcut: ''},
    {name: 'ocq', agent: 'opencode', command: 'ocq --', shortcut: ''},
    {name: 'sol-light', agent: 'codex', shortcut: 'Ctrl+L'},
    {name: 'sol-ultra', agent: 'codex', shortcut: 'Ctrl+U'},
  ];
  const pane = value => settings('pane', {value});
  const choose = name => settings('default', {name});
  const commandState = (commands, defaultName) => state({commands, defaultName, defaultCount: 1});
  await state({...defaults, centered: true, panes: ['General', 'Commands']});
  await pane('Commands');
  await commandState(presets, 'codex');
  const editedStock = {name: 'opencode', agent: 'opencode',
    command: '"$HOME/.local/bin/opencode" --model "fixture/qwen tuned"', shortcut: 'Ctrl+P'};
  await settings('editCommand', editedStock);
  await state({shortcut: ''});
  await settings('focusCommand');
  await recordKey(48, '\t', 0, {direct: true});
  await state({shortcutFocused: true});
  await recordKey(49, ' ', 0, {direct: true});
  await state({shortcutTitle: /press/i});
  await recordKey(35, 'p', control);
  await state({shortcut: editedStock.shortcut, error: ''});
  await settings('save');
  const edited = presets.map(profile => profile.name === editedStock.name ? {...editedStock, agent: ''} : profile);
  await commandState(edited, 'codex');
  await settings('editCommand', {name: editedStock.name});
  await state(editedStock);
  const savedTitle = (await settings('state')).shortcutTitle;
  await settings('record');
  await recordKey(53, '\u001b');
  await state({...editedStock, shortcutTitle: savedTitle});
  await settings('record');
  await recordKey(48, '\t', shift);
  await state({...editedStock, shortcutFocused: false, shellFocused: true, shortcutTitle: savedTitle});
  await recordKey(48, '\t', 0, {direct: true});
  await state({shortcutFocused: true});
  await recordKey(51, '\u007f', 0, {direct: true});
  await state({shortcut: ''});
  await recordKey(53, '\u001b', 0, {direct: true});
  await commandState(edited, 'codex');

  const profile = {name: 'localq', agent: 'opencode', command: 'ocq', shortcut: 'Enter+R'};
  await settings('addCommand', profile);
  await settings('record');
  await state({shortcutFocused: true});
  await recordKey(37, 'l');
  await state({shortcut: '', error: /letter/i});
  await recordKey(35, 'p', control);
  await state({shortcut: '', error: /opencode/i});
  // Retry within the same recording session using a held Return chord.
  await recordKey(36, '\r', 0, {phase: 'down'});
  await recordKey(15, 'r');
  await recordKey(36, '\r', 0, {phase: 'repeat'});
  await recordKey(36, '\r', 0, {phase: 'up'});
  await state({shortcut: profile.shortcut, error: ''});
  await settings('save');
  await commandState([...edited, profile], 'codex');
  await choose(profile.name);
  await commandState([...edited, profile], profile.name);
  const renamed = {...profile, name: 'qwenreview', shortcut: 'Ctrl+Y'};
  await settings('editCommand', {original: profile.name, ...renamed});
  await settings('record');
  await recordKey(16, 'y', control);
  await state({shortcut: renamed.shortcut});
  await settings('save');
  await commandState([...edited, renamed], renamed.name);
  await settings('close');

  // The newly recorded shortcut overrides the default profile at the robot prompt.
  const page = () => $('.terminal-page.selected');
  await key(17, 't', command);
  await expect('Shell ready for the recorded shortcut', () => page()?.dataset.kind === 'shell' && page().dataset.state === 'live');
  await enter('mkdir -p "$HOME/recording" && cd "$HOME/recording" && echo ready');
  await expect('Shortcut test shell has its own directory', () => selected().title.includes('/recording'));
  await key(40, 'k', command);
  await expect('Robot prompt ready for the recorded shortcut', () => document.activeElement === $('#command-input'));
  await controlLetter(35, 'p');
  await expect('Recorded shortcut opens its agent terminal', () => page()?.dataset.kind === 'agent' && page().dataset.state === 'live');
  const recordedTerminal = activeTerminal();
  await expect('Recorded shortcut selects the edited OpenCode command', () =>
    rows(recordedTerminal).includes('Model: fixture/qwen tuned'));
  await enter('/exit');
  await expect('Recorded shortcut session exits', () => page().dataset.state === 'ended');
  await key(13, 'w', command);
  await open();
  await commandState([...edited, renamed], renamed.name);
  await pane('General');
  await settings('mode', {value: 'Agent'});
  await settings('record');
  await state({shortcut: 'Press ⌘ and a letter'});
  await recordKey(17, 't', command);
  await state({shortcut: 'Shortcut in use. Try another.'});
  await recordKey(37, 'l', command);
  await state({shortcut: 'Shortcut in use. Try another.', splitShortcut: '⌘L'});
  await settings('recordSplit');
  await state({shortcut: '⌘K', splitShortcut: 'Press ⌘ and a letter'});
  await recordKey(40, 'k', command);
  await state({splitShortcut: 'Shortcut in use. Try another.'});
  await recordKey(53, '\x1b');
  await state({splitShortcut: '⌘L'});
  await settings('recordSplit');
  await recordKey(38, 'j', command);
  await state({splitShortcut: '⌘J'});
  await settings('record');
  await recordKey(16, 'y', command | option);
  const changed = {mode: 'Agent', shortcut: '⌥⌘Y', splitShortcut: '⌘J'};
  await state(changed);
  await settings('close');
  await expect('New tab and mode toggle reflect saved settings', () =>
    $('#new-tab').getAttribute('aria-label') === 'New agent tab' && $('#command-toggle').title.includes('⌥⌘Y'));
  await expect('Split View button reflects its new shortcut', () =>
    $('#panes-toggle').title.includes('⌘J') && $('#panes-toggle').getAttribute('aria-keyshortcuts') === 'Meta+J');
  if (await key(37, 'l', command)) throw new Error('Previous Split View shortcut still owns Command-L');
  if (!await key(38, 'j', command)) throw new Error('View menu did not receive Command-J');
  await expect('Recorded shortcut enters Split View', () => panes().length === tabs().length && panes().length > 1);
  await key(38, 'j', command);
  await expect('Recorded shortcut returns focus to the selected tab', () =>
    panes().length === 1 && document.activeElement === focusedTerminal());
  $('#start-agent').click();
  await expect('Inspector launcher follows the custom default', () =>
    $('#agent-launch').open && $('#agent-launch-form').elements.agent.value === renamed.name);
  $('#agent-launch').close();
  await open();
  await state(changed);
  await pane('Commands');
  await commandState([...edited, renamed], renamed.name);
  await settings('removeCommand', {name: renamed.name});
  await commandState(edited, editedStock.name);
  await settings('close');
  const before = tabs().length;
  await key(17, 't', command);
  await expect('Command-T honors the default agent tab preference', () =>
    tabs().length === before + 1 && selected().textContent.includes('OpenCode') &&
    page()?.dataset.kind === 'agent' && page().dataset.state === 'live');
  const defaultTerminal = activeTerminal();
  await expect('The edited stock command uses its quoted model argument', () =>
    rows(defaultTerminal).includes('Model: fixture/qwen tuned'));
  await key(16, 'y', command | option);
  await expect('The recorded shortcut switches to the shell', () => page()?.dataset.kind === 'shell');
  await key(16, 'y', command | option);
  await expect('The recorded shortcut restores the live agent', () => page()?.dataset.kind === 'agent');
  await enter('/exit');
  await expect('Offline test agent exited', () => page()?.dataset.state === 'ended');
  await key(13, 'w', command);
  await expect('Settings test tab closed', () => tabs().length === before);
  await open();
  await commandState(edited, editedStock.name);
  await settings('editCommand', {name: 'opencode', command: 'opencode'});
  await settings('clear');
  await state({shortcut: ''});
  await settings('save');
  await choose('codex');
  await commandState(presets, 'codex');
  await pane('General');
  await settings('mode', {value: 'Terminal'});
  await settings('record');
  await recordKey(40, 'k', command);
  await state({shortcut: '⌘K'});
  await settings('recordSplit');
  await recordKey(37, 'l', command);
  await state(defaults);
  await settings('close');
  await expect('Original tab defaults and menu shortcut restored', () =>
    $('#new-tab').getAttribute('aria-label') === 'New terminal tab' && $('#command-toggle').title.includes('(⌘K)'));
}
