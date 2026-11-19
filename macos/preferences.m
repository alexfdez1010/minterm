#import "preferences.h"
#import "agent_commands.h"

NSGridView *MTFormGrid(NSArray<NSArray<NSView *> *> *rows) {
    NSGridView *grid = [NSGridView gridViewWithViews:rows];
    CGFloat labelWidth = 0;
    for (NSArray<NSView *> *row in rows) labelWidth = MAX(labelWidth, row.firstObject.intrinsicContentSize.width);
    [grid columnAtIndex:0].width = ceil(labelWidth);
    [grid columnAtIndex:0].xPlacement = NSGridCellPlacementTrailing;
    [grid columnAtIndex:1].xPlacement = NSGridCellPlacementFill;
    grid.rowAlignment = NSGridRowAlignmentFirstBaseline;
    grid.rowSpacing = 12;
    grid.columnSpacing = 12;
    return grid;
}

static NSView *form(NSArray<NSArray<NSView *> *> *rows) {
    NSView *view = [NSView new];
    NSGridView *grid = MTFormGrid(rows);
    grid.translatesAutoresizingMaskIntoConstraints = NO;
    [view addSubview:grid];
    NSLayoutConstraint *preferredWidth = [grid.widthAnchor constraintEqualToConstant:340];
    preferredWidth.priority = NSLayoutPriorityDefaultHigh;
    [NSLayoutConstraint activateConstraints:@[
        [grid.centerYAnchor constraintEqualToAnchor:view.centerYAnchor],
        [grid.centerXAnchor constraintEqualToAnchor:view.centerXAnchor],
        [grid.leadingAnchor constraintGreaterThanOrEqualToAnchor:view.leadingAnchor constant:18],
        preferredWidth,
    ]];
    return view;
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
    NSPopUpButton *_mode;
    NSButton *_shortcut;
    id _monitor;
    MTAgentCommands *_commands;
}
- (instancetype)initWithChange:(MTChangePreference)change {
    if (!(self = [super init])) return nil;
    _change = change;
    _mode = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    [_mode addItemsWithTitles:@[@"Terminal", @"Agent"]];
    _mode.target = self;
    _mode.action = @selector(modeChanged:);
    _mode.accessibilityLabel = @"New tabs";
    _shortcut = [NSButton buttonWithTitle:@"" target:self action:@selector(record:)];
    _shortcut.accessibilityLabel = @"Record switch mode shortcut";
    _shortcut.toolTip = @"Record a Command shortcut";
    _generalView = form(@[
        @[[NSTextField labelWithString:@"New tabs"], _mode],
        @[[NSTextField labelWithString:@"Switch mode"], _shortcut],
    ]);

    _commands = [[MTAgentCommands alloc] initWithChange:change];
    _commandsView = _commands.view;

    return self;
}

- (void)reload:(NSDictionary *)values {
    [self stopRecording];
    _values = [values mutableCopy];
    [_mode selectItemAtIndex:[values[@"newTabMode"] isEqual:@"agent"] ? 1 : 0];
    [self reloadCommands:values];
    _shortcut.title = values[@"shortcut"];
}

- (void)reloadCommands:(NSDictionary *)values {
    NSString *error = [values[@"agentCommandsError"] isKindOfClass:NSString.class] ? values[@"agentCommandsError"] : nil;
    [_commands reload:values[@"agentCommands"] defaultName:values[@"agent"] error:error];
}

- (BOOL)save:(NSString *)value field:(NSString *)field {
    if (!field || !value) return NO;
    if (![field isEqual:@"shortcut"] && [_values[field] isEqual:value]) return YES;
    char error[1024] = {0};
    if (!_change(field.UTF8String, value.UTF8String, error, sizeof(error))) {
        NSAlert *alert = [NSAlert new];
        alert.messageText = @"Could not save setting";
        alert.informativeText = @(error);
        [alert beginSheetModalForWindow:_generalView.window completionHandler:nil];
        return NO;
    }
    _values[field] = value;
    return YES;
}

- (void)modeChanged:(id)sender {
    [self save:_mode.indexOfSelectedItem == 1 ? @"agent" : @"terminal" field:@"newTabMode"];
}
- (BOOL)cancelCommandEditor { return [_commands cancelEditor]; }

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
