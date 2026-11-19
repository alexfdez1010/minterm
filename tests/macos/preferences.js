// Exercise native Settings and use the selected command in the terminal window.
async function verifyPreferences() {
  const settings = (action, args = {}) => window.webkit.messageHandlers.testKey
    .postMessage({preferences: {action, ...args}});
  function matches(actual, expected) {
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
  const recordKey = (code, letter, modifiers) => settings('key', {code, letter, modifiers});
  const defaults = {mode: 'Terminal', shortcut: '⌘K'};
  const option = 1 << 19;
  await settings('prepare');
  await open();
  const presets = [
    {name: 'codex', title: 'Codex', agent: '', command: 'codex'},
    {name: 'claude', title: 'Claude Code', agent: '', command: 'claude'},
    {name: 'opencode', title: 'OpenCode', agent: '', command: 'opencode'},
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
    command: '"$HOME/.local/bin/opencode" --model "fixture/qwen tuned"', shortcut: ''};
  await settings('editCommand', editedStock);
  const edited = presets.map(profile => profile.name === editedStock.name ? {...editedStock, agent: ''} : profile);
  await commandState(edited, 'codex');
  const reopened = await settings('readCommand', {name: editedStock.name});
  if (!matches(reopened, editedStock)) throw new Error('Reopened stock command: ' + JSON.stringify(reopened));
  const profile = {name: 'localq', agent: 'opencode', command: 'ocq', shortcut: 'Ctrl+R'};
  await settings('addCommand', profile);
  await commandState([...edited, profile], 'codex');
  await choose(profile.name);
  await commandState([...edited, profile], profile.name);
  const renamed = {...profile, name: 'qwenreview', shortcut: 'Ctrl+Y'};
  await settings('editCommand', {original: profile.name, ...renamed});
  await commandState([...edited, renamed], renamed.name);
  await pane('General');
  await settings('mode', {value: 'Agent'});
  await settings('record');
  await state({shortcut: 'Press ⌘ and a letter'});
  await recordKey(17, 't', command);
  await state({shortcut: 'Shortcut in use. Try another.'});
  await recordKey(16, 'y', command | option);
  const changed = {mode: 'Agent', shortcut: '⌥⌘Y'};
  await state(changed);
  await settings('close');
  await expect('New tab and mode toggle reflect saved settings', () =>
    $('#new-tab').getAttribute('aria-label') === 'New agent tab' && $('#command-toggle').title.includes('⌥⌘Y'));
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
  const page = () => $('.terminal-page.selected');
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
  await choose('codex');
  await commandState(presets, 'codex');
  await pane('General');
  await settings('mode', {value: 'Terminal'});
  await settings('record');
  await recordKey(40, 'k', command);
  await state(defaults);
  await settings('close');
  await expect('Original tab defaults and menu shortcut restored', () =>
    $('#new-tab').getAttribute('aria-label') === 'New terminal tab' && $('#command-toggle').title.includes('(⌘K)'));
}
