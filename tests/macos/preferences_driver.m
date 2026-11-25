// AppKit adapter for the Settings window in this test process.
static NSView *qa_setting(NSView *view, NSString *label) {
    if ([view.accessibilityLabel isEqual:label]) return view;
    for (NSView *child in view.subviews) {
        NSView *match = qa_setting(child, label);
        if (match) return match;
    }
    return nil;
}

static void qa_setting_key(NSWindow *window, NSDictionary *request) {
    NSString *letter = request[@"letter"], *phase = request[@"phase"];
    for (NSString *direction in phase.length ? @[phase] : @[@"down", @"up"]) {
        NSEvent *event = [NSEvent keyEventWithType:[direction isEqual:@"up"] ? NSEventTypeKeyUp : NSEventTypeKeyDown
            location:NSZeroPoint modifierFlags:[request[@"modifiers"] unsignedIntegerValue]
            timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber context:nil
            characters:request[@"text"] ?: letter charactersIgnoringModifiers:letter
            isARepeat:[direction isEqual:@"repeat"] keyCode:[request[@"code"] unsignedShortValue]];
        if ([request[@"direct"] boolValue]) {
            // The hidden test app has no key window. Route ordinary sheet controls
            // through the target window; recorded keys use the AppKit event monitor.
            if (event.type == NSEventTypeKeyDown && [window performKeyEquivalent:event]) continue;
            [window sendEvent:event];
        } else {
            [NSApp postEvent:event atStart:NO];
        }
    }
}

static void qa_command_editor(NSWindow *sheet, NSDictionary *request, void (^reply)(id, NSString *)) {
    NSString *action = request[@"action"];
    NSView *content = sheet.contentView;
    NSButton *shortcut = (NSButton *)qa_setting(content, @"Command shortcut");
    if (![shortcut isKindOfClass:NSButton.class]) { reply(nil, @"Missing command shortcut recorder"); return; }
    if ([action isEqual:@"state"]) {
        NSTextField *name = (NSTextField *)qa_setting(content, @"Command name");
        NSTextField *command = (NSTextField *)qa_setting(content, @"Shell command");
        NSPopUpButton *agent = (NSPopUpButton *)qa_setting(content, @"Base agent");
        NSTextField *error = (NSTextField *)qa_setting(content, @"Command error");
        reply(@{@"name": name.stringValue, @"agent": agent.selectedItem.representedObject,
                @"command": command.stringValue, @"shortcut": shortcut.accessibilityValue ?: @"",
                @"shortcutTitle": shortcut.title, @"shortcutFocused": sheet.firstResponder == shortcut ? @YES : @NO,
                @"shellFocused": command.currentEditor && sheet.firstResponder == command.currentEditor ? @YES : @NO,
                @"error": error.stringValue}, nil);
        return;
    }
    if ([action isEqual:@"key"]) {
        qa_setting_key(sheet, request);
    } else if ([action isEqual:@"focusCommand"]) {
        if (![sheet makeFirstResponder:qa_setting(content, @"Shell command")]) { reply(nil, @"Cannot focus shell command"); return; }
    } else {
        NSDictionary *buttons = @{@"record": @"Command shortcut", @"clear": @"Clear command shortcut",
                                  @"save": @"Save"};
        NSString *label = buttons[action];
        NSButton *button = label ? (NSButton *)qa_setting(content, label) : nil;
        if (!button || button.hidden || !button.enabled) { reply(nil, @"Unavailable editor action"); return; }
        [button performClick:nil];
    }
    reply(@YES, nil);
}

// Drive the same table selection, sheet fields and buttons used in Settings.
static void qa_commands(NSWindow *window, NSDictionary *request, void (^reply)(id, NSString *)) {
    NSString *action = request[@"action"];
    NSTableView *table = (NSTableView *)qa_setting(window.contentView, @"Agent commands");
    if (!table) { reply(nil, @"Missing commands list"); return; }
    NSMutableArray *commands = [NSMutableArray array];
    NSInteger selected = -1, defaults = 0;
    NSString *defaultName = @"";
    NSDictionary *ids = @{@"Codex": @"codex", @"Claude Code": @"claude", @"OpenCode": @"opencode", @"Antigravity": @"antigravity"};
    NSString *target = request[@"original"] ?: request[@"name"];
    for (NSInteger row = 0; row < table.numberOfRows; row++) {
        NSMutableDictionary *entry = [NSMutableDictionary dictionary];
        for (NSTableColumn *column in table.tableColumns) {
            id value = [table.dataSource tableView:table objectValueForTableColumn:column row:row];
            entry[column.identifier] = value;
        }
        entry[@"title"] = entry[@"name"];
        entry[@"name"] = ids[entry[@"title"]] ?: entry[@"title"];
        entry[@"agent"] = ids[entry[@"agent"]] ?: @"";
        if ([entry[@"default"] isEqual:@"✓"]) {
            defaultName = entry[@"name"];
            defaults++;
        }
        if ([entry[@"name"] isEqual:target]) selected = row;
        [commands addObject:entry];
    }
    if ([action isEqual:@"state"]) { reply(@{@"commands": commands, @"defaultName": defaultName, @"defaultCount": @(defaults)}, nil); return; }
    BOOL adding = [action isEqual:@"addCommand"];
    NSDictionary *buttons = @{@"addCommand": @"Add command", @"editCommand": @"Edit command",
                              @"removeCommand": @"Remove command", @"default": @"Set as Default"};
    if (!buttons[action]) { reply(nil, @"Unknown commands action"); return; }
    if (!adding) {
        if (selected < 0) { reply(nil, @"Command missing from Settings list"); return; }
        [table selectRowIndexes:[NSIndexSet indexSetWithIndex:selected] byExtendingSelection:NO];
    }
    [(NSButton *)qa_setting(window.contentView, buttons[action]) performClick:nil];
    if (adding || [action isEqual:@"editCommand"]) {
        NSView *content = window.attachedSheet.contentView;
        NSTextField *name = (NSTextField *)qa_setting(content, @"Command name");
        NSTextField *command = (NSTextField *)qa_setting(content, @"Shell command");
        NSPopUpButton *agent = (NSPopUpButton *)qa_setting(content, @"Base agent");
        if (!name || !command || !agent) { reply(nil, @"Missing command sheet"); return; }
        if (request[@"name"]) name.stringValue = request[@"name"];
        if (request[@"command"]) command.stringValue = request[@"command"];
        if (request[@"agent"]) [agent selectItemAtIndex:[agent indexOfItemWithRepresentedObject:request[@"agent"]]];
    }
    reply(@YES, nil);
}

static void qa_preferences(NSWindow *terminal, NSDictionary *request, void (^reply)(id, NSString *)) {
    NSString *action = request[@"action"];
    if ([action isEqual:@"prepare"]) {
        NSRect screen = terminal.screen.visibleFrame;
        [terminal setFrameOrigin:NSMakePoint(NSMinX(screen) + 40, round(NSMidY(screen) - NSHeight(terminal.frame)/2))];
        // Keep Settings invisible as the app positions it before ordering it front.
        [NSNotificationCenter.defaultCenter addObserverForName:NSWindowDidMoveNotification
            object:nil queue:nil usingBlock:^(NSNotification *note) {
                NSWindow *window = note.object;
                if ([window.title isEqual:@"Minterm Settings"]) window.alphaValue = 0;
            }];
        reply(@YES, nil);
        return;
    }
    NSWindow *window = nil;
    for (NSWindow *candidate in NSApp.windows) {
        if (candidate.visible && [candidate.title isEqual:@"Minterm Settings"]) {
            window = candidate;
            break;
        }
    }
    if (!window) {
        if ([action isEqual:@"state"]) reply(@{}, nil);
        else reply(nil, @"Settings window is closed");
        return;
    }
    window.alphaValue = 0;
    if ([action isEqual:@"close"]) {
        [window performClose:nil];
        reply(@YES, nil);
        return;
    }
    NSTabView *panes = nil;
    for (NSView *child in window.contentView.subviews) {
        if ([child isKindOfClass:NSTabView.class]) { panes = (NSTabView *)child; break; }
    }
    if ([action isEqual:@"pane"]) {
        NSString *name = request[@"value"];
        if (!panes || [panes indexOfTabViewItemWithIdentifier:name] == NSNotFound) {
            reply(nil, @"Missing Settings pane");
            return;
        }
        [panes selectTabViewItemWithIdentifier:name];
        reply(@YES, nil);
        return;
    }
    if ([panes.selectedTabViewItem.identifier isEqual:@"Commands"]) {
        if (window.attachedSheet) qa_command_editor(window.attachedSheet, request, reply);
        else qa_commands(window, request, reply);
        return;
    }
    NSPopUpButton *mode = (NSPopUpButton *)qa_setting(window.contentView, @"New tabs");
    NSButton *shortcut = (NSButton *)qa_setting(window.contentView, @"Record switch mode shortcut");
    NSButton *splitShortcut = (NSButton *)qa_setting(window.contentView, @"Record Split View shortcut");
    NSButton *projectsShortcut = (NSButton *)qa_setting(window.contentView, @"Record Open Project shortcut");
    if (!mode || !shortcut || !splitShortcut || !projectsShortcut) {
        reply(nil, @"General pane is missing a control");
        return;
    }
    if ([action isEqual:@"state"]) {
        NSRect available = terminal.screen.visibleFrame;
        CGFloat x = MIN(MAX(NSMidX(terminal.frame) - NSWidth(window.frame)/2, NSMinX(available)), NSMaxX(available) - NSWidth(window.frame));
        CGFloat y = MIN(MAX(NSMidY(terminal.frame) - NSHeight(window.frame)/2, NSMinY(available)), NSMaxY(available) - NSHeight(window.frame));
        BOOL centered = fabs(NSMinX(window.frame) - x) <= 1 && fabs(NSMinY(window.frame) - y) <= 1;
        reply(@{@"mode": mode.titleOfSelectedItem, @"shortcut": shortcut.title,
                @"splitShortcut": splitShortcut.title, @"projectsShortcut": projectsShortcut.title,
                @"centered": @(centered),
                @"panes": [panes.tabViewItems valueForKey:@"label"]}, nil);
        return;
    }
    if ([action isEqual:@"mode"]) {
        NSString *item = request[@"value"];
        if (!item || [mode indexOfItemWithTitle:item] < 0) { reply(nil, @"Unknown tab mode"); return; }
        [mode selectItemWithTitle:item];
        [mode sendAction:mode.action to:mode.target];
    } else if ([action isEqual:@"record"]) {
        [shortcut sendAction:shortcut.action to:shortcut.target];
    } else if ([action isEqual:@"recordSplit"]) {
        [splitShortcut sendAction:splitShortcut.action to:splitShortcut.target];
    } else if ([action isEqual:@"recordProjects"]) {
        [projectsShortcut sendAction:projectsShortcut.action to:projectsShortcut.target];
    } else if ([action isEqual:@"key"]) {
        qa_setting_key(window, request);
    } else {
        reply(nil, @"Unknown Settings test action");
        return;
    }
    reply(@YES, nil);
}
