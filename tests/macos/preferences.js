// Exercise the native General pane and then use its saved settings in the terminal window.
async function verifyPreferences() {
  const settings = (action, args = {}) => window.webkit.messageHandlers.testKey
    .postMessage({preferences: {action, ...args}});
  const state = async (expected) => {
    const deadline = Date.now() + 5000;
    let actual;
    do {
      actual = await settings('state');
      if (Object.entries(expected).every(([k, v]) => JSON.stringify(actual[k]) === JSON.stringify(v))) return;
      await pause(40);
    } while (Date.now() < deadline);
    throw new Error('Settings ' + JSON.stringify(actual) + ', expected ' + JSON.stringify(expected));
  };
  const open = async () => {
    if (!await key(43, ',', command)) throw new Error('Settings menu shortcut was unavailable');
  };
  const recordKey = (code, letter, modifiers) => settings('key', {code, letter, modifiers});
  const defaults = {mode: 'Terminal', agent: 'Codex', shortcut: '⇧⌘K'};
  const option = 1 << 19;
  await settings('prepare');
  await open();
  await state(defaults);
  await settings('agent', {value: 'Codex with Ollama'});
  await settings('mode', {value: 'Agent'});
  await settings('record');
  await state({shortcut: 'Press ⌘ and a letter'});
  await recordKey(17, 't', command);
  await state({shortcut: 'Shortcut in use. Try another.'});
  await recordKey(16, 'y', command | option);
  const changed = {mode: 'Agent', agent: 'Codex with Ollama', shortcut: '⌥⌘Y'};
  await state(changed);
  await settings('close');
  await expect('New tab and mode toggle reflect saved settings', () =>
    $('#new-tab').getAttribute('aria-label') === 'New agent tab' && $('#command-toggle').title.includes('⌥⌘Y'));
  $('#start-agent').click();
  await expect('Inspector launcher follows the Ollama preference', () =>
    $('#agent-launch').open && $('#agent-launch-form').elements.agent.value === 'codex-ollama');
  $('#agent-launch').close();
  await open();
  await state(changed); // show() reloads values from the Rust preferences store.
  await settings('agent', {value: 'Codex'}); // use the offline test CLI for the next tab.
  await settings('close');
  const before = tabs().length;
  await key(17, 't', command);
  const page = () => $('.terminal-page.selected');
  await expect('Command-T honors the default agent tab preference', () =>
    tabs().length === before + 1 && selected().textContent.includes('Codex') &&
    page()?.dataset.kind === 'agent' && page().dataset.state === 'live');
  await key(16, 'y', command | option);
  await expect('The recorded shortcut switches to the shell', () => page()?.dataset.kind === 'shell');
  await key(16, 'y', command | option);
  await expect('The recorded shortcut restores the live agent', () => page()?.dataset.kind === 'agent');
  await enter('/exit');
  await expect('Offline test agent exited', () => page()?.dataset.state === 'ended');
  await key(13, 'w', command);
  await expect('Settings test tab closed', () => tabs().length === before);
  await open();
  await state({...changed, agent: 'Codex'});
  await settings('mode', {value: 'Terminal'});
  await settings('record');
  await recordKey(40, 'K', command | shift);
  await state(defaults);
  await settings('pane', {value: 'Ollama'});
  const models = ['fixture-coder:1b', 'fixture-reasoner:2b'];
  await state({models: models.slice(0, 1)});
  await settings('refresh');
  await state({models});
  await settings('selectModel', {index: 1});
  await settings('close');
  await open();
  await state({model: models[1]});
  await settings('pane', {value: 'General'});
  await state(defaults);
  await settings('close');
  await expect('Original tab defaults and menu shortcut restored', () =>
    $('#new-tab').getAttribute('aria-label') === 'New terminal tab' && $('#command-toggle').title.includes('⇧⌘K'));
}
