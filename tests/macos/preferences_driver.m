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
    NSTableView *table = (NSTableView *)qa_setting(window.contentView, @"Custom commands");
    if (!table) { reply(nil, @"Missing commands list"); return; }
    NSMutableArray *commands = [NSMutableArray array];
    NSInteger selected = -1;
    NSString *target = request[@"original"] ?: request[@"name"];
    for (NSInteger row = 0; row < table.numberOfRows; row++) {
        NSMutableDictionary *entry = [NSMutableDictionary dictionary];
        for (NSTableColumn *column in table.tableColumns) {
            id value = [table.dataSource tableView:table objectValueForTableColumn:column row:row];
            entry[column.identifier] = value;
        }
        entry[@"agent"] = @{@"Codex":@"codex", @"Claude Code":@"claude", @"OpenCode":@"opencode"}[entry[@"agent"]];
        if ([entry[@"name"] isEqual:target]) selected = row;
        [commands addObject:entry];
    }
    if ([action isEqual:@"state"]) { reply(@{@"commands": commands}, nil); return; }
    BOOL adding = [action isEqual:@"addCommand"], removing = [action isEqual:@"removeCommand"];
    if (!(adding || removing || [action isEqual:@"editCommand"])) { reply(nil, @"Unknown commands action"); return; }
    if (!adding) {
        if (selected < 0) { reply(nil, @"Command missing from Settings list"); return; }
        [table selectRowIndexes:[NSIndexSet indexSetWithIndex:selected] byExtendingSelection:NO];
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

static void qa_preferences(NSDictionary *request, void (^reply)(id, NSString *)) {
    NSString *action = request[@"action"];
    if ([action isEqual:@"prepare"]) {
        // The window is centered during construction, before it is ordered front.
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
    if ([panes.selectedTabViewItem.identifier isEqual:@"Codex"]) {
        NSComboBox *model = (NSComboBox *)qa_setting(window.contentView, @"Ollama model");
        NSPopUpButton *backend = (NSPopUpButton *)qa_setting(window.contentView, @"Codex backend");
        if (!model || !backend) { reply(nil, @"Missing Codex backend controls"); return; }
        if ([action isEqual:@"state"]) {
            reply(@{@"model": model.stringValue, @"models": model.objectValues,
                    @"backend": backend.titleOfSelectedItem, @"enabled": @(model.enabled)}, nil);
            return;
        }
        if ([action isEqual:@"backend"]) {
            [backend selectItemWithTitle:request[@"value"]];
            [backend sendAction:backend.action to:backend.target];
        } else if ([action isEqual:@"refresh"]) {
            NSButton *refresh = (NSButton *)qa_setting(window.contentView, @"Refresh");
            if (!refresh) { reply(nil, @"Missing Refresh button"); return; }
            [refresh performClick:nil];
        } else if ([action isEqual:@"selectModel"]) {
            NSInteger index = [request[@"index"] integerValue];
            if (index < 0 || index >= model.numberOfItems) { reply(nil, @"Missing model"); return; }
            [model selectItemAtIndex:index];
        } else {
            reply(nil, @"Unknown model picker test action");
            return;
        }
        reply(@YES, nil);
        return;
    }
    NSPopUpButton *mode = (NSPopUpButton *)qa_setting(window.contentView, @"New tabs");
    NSPopUpButton *agent = (NSPopUpButton *)qa_setting(window.contentView, @"Agent command");
    NSButton *shortcut = (NSButton *)qa_setting(window.contentView, @"Record switch mode shortcut");
    NSTextField *executable = (NSTextField *)qa_setting(window.contentView, @"Executable");
    if (!mode || !agent || !shortcut) {
        reply(nil, @"General pane is missing a control");
        return;
    }
    if ([action isEqual:@"state"]) {
        reply(@{@"mode": mode.titleOfSelectedItem, @"agent": agent.titleOfSelectedItem,
                @"shortcut": shortcut.title, @"agents": agent.itemTitles,
                @"executable": executable.stringValue ?: @"",
                @"panes": [panes.tabViewItems valueForKey:@"label"]}, nil);
        return;
    }
    if ([action isEqual:@"mode"] || [action isEqual:@"agent"]) {
        NSPopUpButton *popup = [action isEqual:@"mode"] ? mode : agent;
        NSString *item = request[@"value"];
        if (!item || [popup indexOfItemWithTitle:item] < 0) { reply(nil, @"Unknown Settings option"); return; }
        [popup selectItemWithTitle:item];
        [popup sendAction:popup.action to:popup.target];
    } else if ([action isEqual:@"draftExecutable"]) {
        if (!executable || ![window makeFirstResponder:executable]) { reply(nil, @"Executable field unavailable"); return; }
        NSTextView *editor = (NSTextView *)executable.currentEditor;
        [editor selectAll:nil];
        [editor insertText:request[@"value"] replacementRange:editor.selectedRange];
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
