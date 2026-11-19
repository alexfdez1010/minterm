#import "preferences.h"

static NSView *form(NSArray<NSArray<NSView *> *> *rows) {
    NSView *view = [NSView new];
    NSGridView *grid = [NSGridView gridViewWithViews:rows];
    [grid columnAtIndex:0].xPlacement = NSGridCellPlacementTrailing;
    [grid columnAtIndex:1].xPlacement = NSGridCellPlacementFill;
    grid.rowAlignment = NSGridRowAlignmentFirstBaseline;
    grid.rowSpacing = 12;
    grid.columnSpacing = 12;
    grid.translatesAutoresizingMaskIntoConstraints = NO;
    [view addSubview:grid];
    [NSLayoutConstraint activateConstraints:@[
        [grid.topAnchor constraintEqualToAnchor:view.topAnchor constant:22],
        [grid.leadingAnchor constraintEqualToAnchor:view.leadingAnchor constant:18],
        [grid.trailingAnchor constraintEqualToAnchor:view.trailingAnchor constant:-20],
    ]];
    return view;
}

static NSStackView *horizontal(NSArray<NSView *> *views) {
    NSStackView *stack = [NSStackView stackViewWithViews:views];
    stack.orientation = NSUserInterfaceLayoutOrientationHorizontal;
    stack.spacing = 6;
    return stack;
}

static BOOL shortcutTaken(NSMenu *menu, NSString *letter, NSEventModifierFlags flags) {
    for (NSMenuItem *item in menu.itemArray) {
        if (item.submenu && shortcutTaken(item.submenu, letter, flags)) return YES;
        NSString *key = item.keyEquivalent;
        if (![key.lowercaseString isEqual:letter]) continue;
        NSEventModifierFlags itemFlags = item.keyEquivalentModifierMask;
        if (![key isEqual:key.lowercaseString]) itemFlags |= NSEventModifierFlagShift;
        if (itemFlags == flags) return YES;
    }
    return NO;
}

@implementation MTPreferencesPane {
    MTChangePreference _change;
    NSMutableDictionary *_values;
    NSPopUpButton *_mode, *_agent, *_backend;
    NSButton *_shortcut, *_refresh;
    NSTextField *_executable, *_host, *_catalogStatus;
    NSComboBox *_model;
    id _monitor;
    NSURLSessionDataTask *_modelRequest;
    NSUInteger _catalogGeneration;
}
- (instancetype)initWithChange:(MTChangePreference)change {
    if (!(self = [super init])) return nil;
    _change = change;
    _mode = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    [_mode addItemsWithTitles:@[@"Terminal", @"Agent"]];
    _mode.target = self;
    _mode.action = @selector(modeChanged:);
    _mode.accessibilityLabel = @"New tabs";
    _agent = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    [_agent addItemsWithTitles:@[@"Codex", @"Claude Code", @"OpenCode"]];
    _agent.target = self;
    _agent.action = @selector(agentChanged:);
    _agent.accessibilityLabel = @"Agent command";
    _shortcut = [NSButton buttonWithTitle:@"" target:self action:@selector(record:)];
    _shortcut.accessibilityLabel = @"Record switch mode shortcut";
    _shortcut.toolTip = @"Record a Command shortcut";
    _executable = [NSTextField textFieldWithString:@""];
    _executable.placeholderString = @"Find on PATH";
    _executable.delegate = self;
    _executable.accessibilityLabel = @"Executable";
    NSButton *choose = [NSButton buttonWithTitle:@"Choose…" target:self action:@selector(choose:)];
    _generalView = form(@[
        @[[NSTextField labelWithString:@"New tabs"], _mode],
        @[[NSTextField labelWithString:@"Agent command"], _agent],
        @[[NSTextField labelWithString:@"Executable"], horizontal(@[_executable,choose])],
        @[[NSTextField labelWithString:@"Switch mode"], _shortcut],
    ]);

    _backend = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    [_backend addItemsWithTitles:@[@"Codex default", @"Ollama"]];
    _backend.accessibilityLabel = @"Codex backend";
    _backend.target = self;
    _backend.action = @selector(backendChanged:);
    _host = [NSTextField textFieldWithString:@""];
    _host.accessibilityLabel = @"Ollama server";
    _host.delegate = self;
    _model = [[NSComboBox alloc] initWithFrame:NSZeroRect];
    _model.accessibilityLabel = @"Ollama model";
    _model.placeholderString = @"First installed model";
    _model.delegate = self;
    _refresh = [NSButton buttonWithTitle:@"Refresh" target:self action:@selector(refreshModels)];
    _catalogStatus = [NSTextField wrappingLabelWithString:@""];
    _catalogStatus.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
    _catalogStatus.textColor = NSColor.secondaryLabelColor;
    _catalogStatus.hidden = YES;
    _codexView = form(@[
        @[[NSTextField labelWithString:@"Backend"], _backend],
        @[[NSTextField labelWithString:@"Ollama server"], _host],
        @[[NSTextField labelWithString:@"Model"], horizontal(@[_model,_refresh])],
        @[[NSGridCell emptyContentView], _catalogStatus],
    ]);
    return self;
}

- (NSString *)executableField {
    return @[@"codexPath", @"claudePath", @"opencodePath"][_agent.indexOfSelectedItem];
}

- (void)reload:(NSDictionary *)values {
    [self stopRecording];
    _values = [values mutableCopy];
    [_mode selectItemAtIndex:[values[@"newTabMode"] isEqual:@"agent"] ? 1 : 0];
    NSInteger index = [@[@"codex",@"claude",@"opencode"] indexOfObject:values[@"agent"]];
    [_agent selectItemAtIndex:index == NSNotFound ? 0 : index];
    _shortcut.title = values[@"shortcut"];
    _executable.stringValue = values[self.executableField] ?: @"";
    _host.stringValue = values[@"ollamaHost"];
    _model.stringValue = values[@"ollamaModel"] ?: @"";
    [_backend selectItemAtIndex:[values[@"codexOllama"] boolValue] ? 1 : 0];
    [self updateBackendControls];
}

- (BOOL)save:(NSString *)value field:(NSString *)field {
    if (![field isEqual:@"shortcut"] && [_values[field] isEqual:value]) return YES;
    char error[1024] = {0};
    if (!_change(field.UTF8String, value.UTF8String, error, sizeof(error))) {
        NSAlert *alert = [NSAlert new];
        alert.messageText = @"Could not save setting";
        alert.informativeText = @(error);
        [alert beginSheetModalForWindow:_generalView.window ?: _codexView.window completionHandler:nil];
        return NO;
    }
    _values[field] = value;
    return YES;
}

- (void)modeChanged:(id)sender {
    [self save:_mode.indexOfSelectedItem == 1 ? @"agent" : @"terminal" field:@"newTabMode"];
}
- (void)agentChanged:(id)sender {
    [self save:@[@"codex",@"claude",@"opencode"][_agent.indexOfSelectedItem] field:@"agent"];
    _executable.stringValue = _values[self.executableField] ?: @"";
}
- (void)updateBackendControls {
    BOOL enabled = _backend.indexOfSelectedItem == 1;
    _host.enabled = enabled;
    _model.enabled = enabled;
    _refresh.enabled = enabled;
    if (!enabled) {
        ++_catalogGeneration;
        [_modelRequest cancel];
        _catalogStatus.hidden = YES;
    }
}
- (void)backendChanged:(id)sender {
    BOOL enabled = _backend.indexOfSelectedItem == 1;
    if (![self save:enabled ? @"true" : @"false" field:@"codexOllama"]) {
        [_backend selectItemAtIndex:[_values[@"codexOllama"] boolValue] ? 1 : 0];
    }
    [self updateBackendControls];
    [self refreshModels];
}
- (void)choose:(id)sender {
    NSOpenPanel *panel = [NSOpenPanel openPanel];
    panel.canChooseFiles = YES;
    panel.canChooseDirectories = NO;
    NSString *field = self.executableField;
    [panel beginSheetModalForWindow:_generalView.window completionHandler:^(NSModalResponse result) {
        if (result == NSModalResponseOK && [self save:panel.URL.path field:field])
            self->_executable.stringValue = panel.URL.path;
    }];
}
- (void)controlTextDidEndEditing:(NSNotification *)note {
    NSTextField *field = note.object;
    NSString *key = field == _host ? @"ollamaHost" : field == _model ? @"ollamaModel" : self.executableField;
    if (field == _host) {
        NSString *host = [field.stringValue stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
        while ([host hasSuffix:@"/"]) host = [host substringToIndex:host.length - 1];
        field.stringValue = [[NSURLComponents componentsWithString:host].path hasSuffix:@"/v1"] ? [host substringToIndex:host.length - 3] : host;
    }
    if (![self save:field.stringValue field:key]) field.stringValue = _values[key] ?: @"";
    if (field == _host) [self refreshModels];
}
- (void)comboBoxSelectionDidChange:(NSNotification *)note {
    NSString *model = _model.objectValueOfSelectedItem;
    if (model) [self save:model field:@"ollamaModel"];
}

- (void)refreshModels {
    if (_backend.indexOfSelectedItem != 1) return;
    NSUInteger generation = ++_catalogGeneration;
    [_modelRequest cancel];
    NSString *host = _values[@"ollamaHost"];
    NSString *base = [host stringByTrimmingCharactersInSet:[NSCharacterSet characterSetWithCharactersInString:@"/"]];
    NSURL *url = [NSURL URLWithString:[base stringByAppendingString:@"/api/tags"]];
    if (!url) return;
    _catalogStatus.stringValue = @"Loading models…";
    _catalogStatus.hidden = NO;
    NSURLRequest *request = [NSURLRequest requestWithURL:url cachePolicy:NSURLRequestReloadIgnoringLocalCacheData timeoutInterval:4];
    _modelRequest = [NSURLSession.sharedSession dataTaskWithRequest:request completionHandler:^(NSData *data, NSURLResponse *reply, NSError *error) {
        if (error.code == NSURLErrorCancelled) return;
        NSInteger status = [(NSHTTPURLResponse *)reply statusCode];
        id catalog = status >= 200 && status < 300 && data.length && data.length <= 1024*1024 ? [NSJSONSerialization JSONObjectWithData:data options:0 error:NULL] : nil;
        id models = [catalog isKindOfClass:NSDictionary.class] ? catalog[@"models"] : nil;
        NSMutableArray *names = [NSMutableArray array];
        if ([models isKindOfClass:NSArray.class]) for (id model in models) {
            NSString *name = [model isKindOfClass:NSDictionary.class] ? model[@"name"] : nil;
            if ([name isKindOfClass:NSString.class] && name.length) [names addObject:name];
        }
        dispatch_async(dispatch_get_main_queue(), ^{
            if (generation != self->_catalogGeneration || ![self->_values[@"ollamaHost"] isEqual:host]) return;
            [self->_model removeAllItems];
            [self->_model addItemsWithObjectValues:names];
            self->_catalogStatus.hidden = names.count > 0;
            if (!names.count) self->_catalogStatus.stringValue = [models isKindOfClass:NSArray.class]
                ? @"No models installed. Pull a model in Ollama, then refresh."
                : @"Could not load models. Check the server address and refresh.";
        });
    }];
    [_modelRequest resume];
}

- (void)stopRecording {
    if (_monitor) [NSEvent removeMonitor:_monitor];
    _monitor = nil;
    _shortcut.title = _values[@"shortcut"] ?: @"";
}
- (void)record:(id)sender {
    if (_monitor) { [self stopRecording]; return; }
    _shortcut.title = @"Press ⌘ and a letter";
    __weak MTPreferencesPane *weakSelf = self;
    _monitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyDown handler:^NSEvent *(NSEvent *event) {
        MTPreferencesPane *pane = weakSelf;
        if (!pane || event.window != pane->_generalView.window) return event;
        if (event.keyCode == 53) { [pane stopRecording]; return nil; }
        NSEventModifierFlags mask = NSEventModifierFlagCommand | NSEventModifierFlagOption | NSEventModifierFlagControl | NSEventModifierFlagShift;
        NSEventModifierFlags flags = event.modifierFlags & mask;
        NSString *letter = event.charactersIgnoringModifiers.lowercaseString;
        if (!(flags & NSEventModifierFlagCommand) || letter.length != 1 ||
                [letter characterAtIndex:0] < 'a' || [letter characterAtIndex:0] > 'z') { NSBeep(); return nil; }
        NSString *key = [NSString stringWithFormat:@"%@%@%@⌘%@", flags & NSEventModifierFlagControl ? @"⌃" : @"",
            flags & NSEventModifierFlagOption ? @"⌥" : @"", flags & NSEventModifierFlagShift ? @"⇧" : @"", letter.uppercaseString];
        if (![pane->_values[@"shortcut"] isEqual:key] && shortcutTaken(NSApp.mainMenu,letter,flags)) {
            pane->_shortcut.title = @"Shortcut in use. Try another.";
            return nil;
        }
        [pane save:key field:@"shortcut"];
        [pane stopRecording];
        return nil;
    }];
}
@end
