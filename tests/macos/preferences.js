// Exercise the native General pane and then use its saved settings in the terminal window.
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
  const defaults = {mode: 'Terminal', agent: 'Codex', shortcut: '⌘K'};
  const option = 1 << 19;
  await settings('prepare');
  await open();
  const presets = [
    {name: 'ocq', agent: 'opencode', command: 'ocq --', shortcut: ''},
    {name: 'sol-light', agent: 'codex', shortcut: 'Enter+L'},
    {name: 'sol-ultra', agent: 'codex', shortcut: 'Enter+U'},
  ];
  const baseChoices = ['Codex', 'Claude Code', 'OpenCode', ...presets.map(p => p.name)];
  await state({...defaults, agents: baseChoices, panes: ['General', 'Commands', 'Codex']});
  await settings('pane', {value: 'Commands'});
  await state({commands: presets});
  const profile = {name: 'localq', agent: 'opencode', command: 'ocq', shortcut: 'Enter+R'};
  await settings('addCommand', profile);
  await state({commands: [...presets, profile]});
  await settings('pane', {value: 'General'});
  await settings('agent', {value: profile.name});
  await state({agent: profile.name});
  await settings('pane', {value: 'Commands'});
  const renamed = {name: 'qwenreview', agent: 'opencode', command: '"$HOME/.local/bin/opencode" --model "fixture/qwen tuned"', shortcut: 'Enter+Y'};
  await settings('editCommand', {original: profile.name, ...renamed});
  await state({commands: [...presets, renamed]});
  await settings('pane', {value: 'General'});
  await state({agent: renamed.name, agents: [...baseChoices, renamed.name]});
  await settings('mode', {value: 'Agent'});
  await settings('record');
  await state({shortcut: 'Press ⌘ and a letter'});
  await recordKey(17, 't', command);
  await state({shortcut: 'Shortcut in use. Try another.'});
  await recordKey(16, 'y', command | option);
  const changed = {mode: 'Agent', agent: renamed.name, shortcut: '⌥⌘Y'};
  await state(changed);
  await settings('close');
  await expect('New tab and mode toggle reflect saved settings', () =>
    $('#new-tab').getAttribute('aria-label') === 'New agent tab' && $('#command-toggle').title.includes('⌥⌘Y'));
  $('#start-agent').click();
  await expect('Inspector launcher follows the custom default', () =>
    $('#agent-launch').open && $('#agent-launch-form').elements.agent.value === renamed.name);
  $('#agent-launch').close();
  await open();
  await state(changed); // show() reloads values from the Rust preferences store.
  await settings('close');
  const before = tabs().length;
  await key(17, 't', command);
  const page = () => $('.terminal-page.selected');
  await expect('Command-T honors the default agent tab preference', () =>
    tabs().length === before + 1 && selected().textContent.includes(renamed.name) &&
    page()?.dataset.kind === 'agent' && page().dataset.state === 'live');
  const defaultTerminal = activeTerminal();
  await expect('The default command uses its quoted model argument', () =>
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
  await state(changed);
  await settings('pane', {value: 'Commands'});
  await state({commands: [...presets, renamed]});
  await settings('removeCommand', {name: renamed.name});
  await state({commands: presets});
  await settings('pane', {value: 'General'});
  await state({agent: 'OpenCode', agents: baseChoices});
  await settings('agent', {value: 'Codex'});
  await settings('mode', {value: 'Terminal'});
  await settings('record');
  await recordKey(40, 'k', command);
  await state(defaults);
  await settings('pane', {value: 'Codex'});
  await state({backend: 'Codex default', enabled: false});
  await settings('backend', {value: 'Ollama'});
  const models = ['fixture-coder:1b', 'fixture-reasoner:2b'];
  await state({models: models.slice(0, 1)});
  await settings('refresh');
  await state({models});
  await settings('selectModel', {index: 1});
  await settings('close');
  await open();
  await state({backend: 'Ollama', enabled: true, model: models[1]});
  await settings('backend', {value: 'Codex default'});
  await state({enabled: false, model: models[1]});
  await settings('pane', {value: 'General'});
  await state(defaults);
  // Hiding the executable row ends editing. The unfinished path belongs to the
  // built-in that owned the field, even after the default popup changed.
  const path = '/fixture/tools/custom codex';
  await settings('draftExecutable', {value: path});
  await settings('agent', {value: 'ocq'});
  await state({agent: 'ocq'});
  await settings('agent', {value: 'Codex'});
  await state({...defaults, executable: path});
  await settings('draftExecutable', {value: ''});
  await settings('close');
  await expect('Original tab defaults and menu shortcut restored', () =>
    $('#new-tab').getAttribute('aria-label') === 'New terminal tab' && $('#command-toggle').title.includes('(⌘K)'));
}
