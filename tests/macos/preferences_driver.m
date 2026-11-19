// AppKit adapter for the Settings window in this test process.
static NSView *qa_setting(NSView *view, NSString *label) {
    if ([view.accessibilityLabel isEqual:label]) return view;
    for (NSView *child in view.subviews) {
        NSView *match = qa_setting(child, label);
        if (match) return match;
    }
    return nil;
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
    if (!mode || !agent || !shortcut) {
        reply(nil, @"General pane is missing a control");
        return;
    }
    if ([action isEqual:@"state"]) {
        reply(@{@"mode": mode.titleOfSelectedItem, @"agent": agent.titleOfSelectedItem,
                @"shortcut": shortcut.title, @"agents": agent.itemTitles,
                @"panes": [panes.tabViewItems valueForKey:@"label"]}, nil);
        return;
    }
    if ([action isEqual:@"mode"] || [action isEqual:@"agent"]) {
        NSPopUpButton *popup = [action isEqual:@"mode"] ? mode : agent;
        NSString *item = request[@"value"];
        if (!item || [popup indexOfItemWithTitle:item] < 0) { reply(nil, @"Unknown Settings option"); return; }
        [popup selectItemWithTitle:item];
        [popup sendAction:popup.action to:popup.target];
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
