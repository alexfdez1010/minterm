// AppKit adapter for the Settings window in this test process.
static NSView *qa_setting(NSView *view, NSString *label) {
    if ([view.accessibilityLabel isEqual:label]) return view;
    for (NSView *child in view.subviews) {
        NSView *match = qa_setting(child, label);
        if (match) return match;
    }
    return nil;
}

// Drive the same table selection, sheet fields and buttons used in Settings.
static void qa_commands(NSWindow *window, NSDictionary *request, void (^reply)(id, NSString *)) {
    NSString *action = request[@"action"];
    NSTableView *table = (NSTableView *)qa_setting(window.contentView, @"Agent commands");
    if (!table) { reply(nil, @"Missing commands list"); return; }
    NSMutableArray *commands = [NSMutableArray array];
    NSInteger selected = -1, defaults = 0;
    NSString *defaultName = @"";
    NSDictionary *ids = @{@"Codex": @"codex", @"Claude Code": @"claude", @"OpenCode": @"opencode"};
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
    BOOL adding = [action isEqual:@"addCommand"], removing = [action isEqual:@"removeCommand"];
    BOOL reading = [action isEqual:@"readCommand"], setDefault = [action isEqual:@"default"];
    if (!(adding || removing || reading || setDefault || [action isEqual:@"editCommand"])) { reply(nil, @"Unknown commands action"); return; }
    if (!adding) {
        if (selected < 0) { reply(nil, @"Command missing from Settings list"); return; }
        [table selectRowIndexes:[NSIndexSet indexSetWithIndex:selected] byExtendingSelection:NO];
    }
    if (setDefault) {
        [(NSButton *)qa_setting(window.contentView, @"Set as Default") performClick:nil];
        reply(@YES, nil);
        return;
    }
    NSString *label = adding ? @"Add command" : removing ? @"Remove command" : @"Edit command";
    [(NSButton *)qa_setting(window.contentView, label) performClick:nil];
    if (!removing) {
        NSWindow *sheet = window.attachedSheet;
        NSTextField *name = (NSTextField *)qa_setting(sheet.contentView, @"Command name");
        NSTextField *command = (NSTextField *)qa_setting(sheet.contentView, @"Shell command");
        NSPopUpButton *agent = (NSPopUpButton *)qa_setting(sheet.contentView, @"Base agent");
        NSTextField *shortcut = (NSTextField *)qa_setting(sheet.contentView, @"Command shortcut");
        if (!name || !command || !agent || !shortcut) { reply(nil, @"Missing command sheet"); return; }
        if (reading) {
            NSDictionary *fields = @{@"name": name.stringValue, @"agent": agent.selectedItem.representedObject,
                                     @"command": command.stringValue, @"shortcut": shortcut.stringValue};
            [(NSButton *)qa_setting(sheet.contentView, @"Cancel") performClick:nil];
            reply(fields, nil);
            return;
        }
        if (request[@"name"]) name.stringValue = request[@"name"];
        if (request[@"command"]) command.stringValue = request[@"command"];
        if (request[@"shortcut"]) shortcut.stringValue = request[@"shortcut"];
        if (request[@"agent"]) [agent selectItemAtIndex:[agent indexOfItemWithRepresentedObject:request[@"agent"]]];
        [(NSButton *)qa_setting(sheet.contentView, @"Save") performClick:nil];
        NSTextField *error = (NSTextField *)qa_setting(sheet.contentView, @"Command error");
        if (error.stringValue.length) {
            [(NSButton *)qa_setting(sheet.contentView, @"Cancel") performClick:nil];
            reply(nil, error.stringValue);
            return;
        }
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
        qa_commands(window, request, reply);
        return;
    }
    NSPopUpButton *mode = (NSPopUpButton *)qa_setting(window.contentView, @"New tabs");
    NSButton *shortcut = (NSButton *)qa_setting(window.contentView, @"Record switch mode shortcut");
    if (!mode || !shortcut) {
        reply(nil, @"General pane is missing a control");
        return;
    }
    if ([action isEqual:@"state"]) {
        NSRect available = terminal.screen.visibleFrame;
        CGFloat x = MIN(MAX(NSMidX(terminal.frame) - NSWidth(window.frame)/2, NSMinX(available)), NSMaxX(available) - NSWidth(window.frame));
        CGFloat y = MIN(MAX(NSMidY(terminal.frame) - NSHeight(window.frame)/2, NSMinY(available)), NSMaxY(available) - NSHeight(window.frame));
        BOOL centered = fabs(NSMinX(window.frame) - x) <= 1 && fabs(NSMinY(window.frame) - y) <= 1;
        reply(@{@"mode": mode.titleOfSelectedItem, @"shortcut": shortcut.title, @"centered": @(centered),
                @"panes": [panes.tabViewItems valueForKey:@"label"]}, nil);
        return;
    }
    if ([action isEqual:@"mode"]) {
        NSString *item = request[@"value"];
        if (!item || [mode indexOfItemWithTitle:item] < 0) { reply(nil, @"Unknown tab mode"); return; }
        [mode selectItemWithTitle:item];
        [mode sendAction:mode.action to:mode.target];
    } else if ([action isEqual:@"record"]) {
        [shortcut performClick:nil];
    } else if ([action isEqual:@"key"]) {
        NSString *letter = request[@"letter"];
        NSEvent *event = [NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint
            modifierFlags:[request[@"modifiers"] unsignedIntegerValue] timestamp:NSProcessInfo.processInfo.systemUptime
            windowNumber:window.windowNumber context:nil characters:letter charactersIgnoringModifiers:letter
            isARepeat:NO keyCode:[request[@"code"] unsignedShortValue]];
        // The recorder is a real AppKit local event monitor, scoped to this window.
        [NSApp postEvent:event atStart:YES];
    } else {
        reply(nil, @"Unknown Settings test action");
        return;
    }
    reply(@YES, nil);
}
