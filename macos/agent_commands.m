#import "agent_commands.h"

@interface MTCommandCell : NSTextFieldCell
@end

@implementation MTCommandCell
- (void)drawInteriorWithFrame:(NSRect)frame inView:(NSView *)view {
    CGFloat height = MIN(NSHeight(frame), self.cellSize.height);
    frame.origin.y += floor((NSHeight(frame) - height) / 2);
    frame.size.height = height;
    [super drawInteriorWithFrame:frame inView:view];
}
@end

static NSString *agentTitle(NSString *agent) {
    return @{@"codex": @"Codex", @"claude": @"Claude Code", @"opencode": @"OpenCode"}[agent] ?: agent;
}

static void addBaseAgents(NSPopUpButton *popup) {
    for (NSString *agent in @[@"codex", @"claude", @"opencode"]) {
        [popup addItemWithTitle:agentTitle(agent)];
        popup.lastItem.representedObject = agent;
    }
}

@implementation MTAgentCommands {
    MTChangePreference _change;
    NSString *_defaultName;
    NSArray<NSDictionary *> *_commands;
    NSTableView *_table;
    NSButton *_add, *_remove, *_edit, *_setDefault;
    NSPanel *_sheet;
    NSTextField *_name, *_command, *_shortcut, *_error, *_configError;
    NSPopUpButton *_agent;
    NSDictionary *_previous;
}

- (instancetype)initWithChange:(MTChangePreference)change {
    if (!(self = [super init])) return nil;
    _change = change;
    _view = [NSView new];
    _table = [[NSTableView alloc] initWithFrame:NSMakeRect(0, 0, 480, 150)];
    _table.accessibilityLabel = @"Agent commands";
    _table.dataSource = self;
    _table.delegate = self;
    _table.target = self;
    _table.doubleAction = @selector(editCommand:);
    _table.allowsColumnReordering = NO;
    _table.usesAlternatingRowBackgroundColors = YES;
    NSDictionary *widths = @{@"default": @44, @"name": @82, @"agent": @82, @"shortcut": @62, @"command": @170};
    for (NSString *key in @[@"default", @"name", @"agent", @"shortcut", @"command"]) {
        NSTableColumn *column = [[NSTableColumn alloc] initWithIdentifier:key];
        column.title = key.capitalizedString;
        column.minWidth = 44;
        column.width = [widths[key] doubleValue];
        MTCommandCell *cell = [[MTCommandCell alloc] initTextCell:@""];
        cell.font = [column.dataCell font];
        cell.usesSingleLineMode = YES;
        cell.lineBreakMode = NSLineBreakByTruncatingTail;
        if ([key isEqual:@"default"]) cell.alignment = NSTextAlignmentCenter;
        column.dataCell = cell;
        if (![key isEqual:@"command"]) column.resizingMask = NSTableColumnUserResizingMask;
        [_table addTableColumn:column];
    }
    _table.columnAutoresizingStyle = NSTableViewLastColumnOnlyAutoresizingStyle;
    NSScrollView *scroll = [NSScrollView new];
    scroll.hasVerticalScroller = YES;
    scroll.autohidesScrollers = YES;
    scroll.documentView = _table;
    scroll.borderType = NSBezelBorder;

    _add = [NSButton buttonWithImage:[NSImage imageWithSystemSymbolName:@"plus" accessibilityDescription:@"Add command"]
                                       target:self action:@selector(addCommand:)];
    _remove = [NSButton buttonWithImage:[NSImage imageWithSystemSymbolName:@"minus" accessibilityDescription:@"Remove command"]
                                 target:self action:@selector(removeCommand:)];
    _add.accessibilityLabel = _add.toolTip = @"Add command";
    _remove.accessibilityLabel = _remove.toolTip = @"Remove command";
    _edit = [NSButton buttonWithTitle:@"Edit…" target:self action:@selector(editCommand:)];
    _edit.accessibilityLabel = @"Edit command";

    _setDefault = [NSButton buttonWithTitle:@"Set as Default" target:self action:@selector(setDefault:)];
    NSButton *revealButton = [NSButton buttonWithTitle:@"Show in Finder" target:self action:@selector(reveal:)];
    revealButton.toolTip = @"Show the agent commands file in Finder";
    _configError = [NSTextField wrappingLabelWithString:@""];
    _configError.textColor = NSColor.systemRedColor;
    _configError.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
    _configError.maximumNumberOfLines = 4;
    NSStackView *list = [NSStackView stackViewWithViews:@[_configError, scroll]];
    list.orientation = NSUserInterfaceLayoutOrientationVertical;
    list.alignment = NSLayoutAttributeWidth;
    NSStackView *buttons = [NSStackView stackViewWithViews:@[_add, _remove, revealButton]];
    buttons.spacing = 4;
    NSStackView *actions = [NSStackView stackViewWithViews:@[_setDefault, _edit]];
    actions.spacing = 6;
    for (NSView *item in @[list, buttons, actions]) {
        item.translatesAutoresizingMaskIntoConstraints = NO;
        [_view addSubview:item];
    }
    [NSLayoutConstraint activateConstraints:@[
        [list.topAnchor constraintEqualToAnchor:_view.topAnchor constant:14],
        [list.leadingAnchor constraintEqualToAnchor:_view.leadingAnchor constant:16],
        [list.trailingAnchor constraintEqualToAnchor:_view.trailingAnchor constant:-16],
        [list.bottomAnchor constraintEqualToAnchor:buttons.topAnchor constant:-8],
        [buttons.leadingAnchor constraintEqualToAnchor:list.leadingAnchor],
        [buttons.bottomAnchor constraintEqualToAnchor:_view.bottomAnchor constant:-14],
        [actions.trailingAnchor constraintEqualToAnchor:list.trailingAnchor],
        [actions.centerYAnchor constraintEqualToAnchor:buttons.centerYAnchor],
    ]];
    return self;
}

- (NSString *)change:(const char *)field value:(NSString *)value {
    char error[1024] = {0};
    return _change(field, value.UTF8String, error, sizeof(error)) ? nil : @(error);
}

- (NSString *)edit:(NSDictionary *)edit {
    NSData *data = [NSJSONSerialization dataWithJSONObject:edit options:0 error:NULL];
    NSString *json = [[NSString alloc] initWithData:data encoding:NSUTF8StringEncoding];
    return json ? [self change:"agentCommand" value:json] : @"Could not save the command.";
}

- (void)reveal:(id)sender {
    NSString *message = [self change:"revealAgentCommands" value:@""];
    if (message) [self presentError:message title:@"Could not show the commands file"];
}
- (void)presentError:(NSString *)message title:(NSString *)title {
    NSAlert *alert = [NSAlert new];
    alert.messageText = title;
    alert.informativeText = message;
    [alert beginSheetModalForWindow:self.view.window completionHandler:nil];
}

- (void)reload:(NSArray<NSDictionary *> *)commands defaultName:(NSString *)name error:(NSString *)error {
    NSString *selection = self.selectedCommand[@"name"];
    _defaultName = [name copy];
    _configError.hidden = !error.length;
    _configError.stringValue = error ?: @"";
    _add.enabled = !error.length;
    _commands = [commands copy];
    [_table reloadData];
    NSArray *names = [_commands valueForKey:@"name"];
    NSUInteger row = [names indexOfObject:selection ?: _defaultName];
    if (row == NSNotFound) row = [names indexOfObject:_defaultName];
    if (row != NSNotFound) [_table selectRowIndexes:[NSIndexSet indexSetWithIndex:row] byExtendingSelection:NO];
    [self updateButtons];
}

- (NSInteger)numberOfRowsInTableView:(NSTableView *)table { return _commands.count; }

- (id)tableView:(NSTableView *)table objectValueForTableColumn:(NSTableColumn *)column row:(NSInteger)row {
    NSDictionary *entry = _commands[row];
    NSString *key = column.identifier;
    if ([key isEqual:@"default"]) return [entry[@"name"] isEqual:_defaultName] ? @"✓" : @"";
    if ([key isEqual:@"name"]) return agentTitle(entry[key]);
    if ([key isEqual:@"agent"]) return [entry[@"name"] isEqual:entry[key]] ? @"" : agentTitle(entry[key]);
    return entry[key] ?: @"";
}

- (NSString *)tableView:(NSTableView *)table toolTipForCell:(NSCell *)cell rect:(NSRectPointer)rect
            tableColumn:(NSTableColumn *)column row:(NSInteger)row mouseLocation:(NSPoint)mouse {
    if ([column.identifier isEqual:@"shortcut"]) {
        NSString *shortcut = _commands[row][@"shortcut"];
        if ([shortcut hasPrefix:@"Enter+"]) {
            return [NSString stringWithFormat:@"Also available as Ctrl+%@", [shortcut substringFromIndex:6]];
        }
    }
    return [column.identifier isEqual:@"command"] ? _commands[row][@"command"] : nil;
}

- (void)tableViewSelectionDidChange:(NSNotification *)notification { [self updateButtons]; }

- (NSDictionary *)selectedCommand {
    NSInteger row = _table.selectedRow;
    return row >= 0 && row < (NSInteger)_commands.count ? _commands[row] : nil;
}
- (void)updateButtons {
    NSDictionary *entry = self.selectedCommand;
    _edit.enabled = entry != nil;
    _remove.enabled = entry && _commands.count > 1;
    _setDefault.enabled = entry && ![entry[@"name"] isEqual:_defaultName];
}

- (void)setDefault:(id)sender {
    if (!_setDefault.enabled) return;
    NSString *message = [self change:"agent" value:self.selectedCommand[@"name"]];
    if (message) [self presentError:message title:@"Could not set default agent"];
}

- (void)addCommand:(id)sender { [self showEditor:nil]; }
- (void)editCommand:(id)sender {
    if (_edit.enabled) [self showEditor:_commands[_table.selectedRow]];
}

- (void)removeCommand:(id)sender {
    if (!_remove.enabled) return;
    NSString *message = [self edit:@{@"previous": self.selectedCommand}];
    if (message) [self presentError:message title:@"Could not remove command"];
}

- (void)showEditor:(NSDictionary *)previous {
    if (_sheet) return;
    _previous = previous;
    CGFloat width = NSWidth(_view.window.contentView.bounds) - 32;
    _sheet = [[NSPanel alloc] initWithContentRect:NSMakeRect(0, 0, width, 228)
        styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
    _sheet.title = previous ? @"Edit Command" : @"Add Command";
    _name = [NSTextField textFieldWithString:previous[@"name"] ?: @""];
    _name.accessibilityLabel = @"Command name";
    _name.placeholderString = @"ocq";
    _agent = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    _agent.accessibilityLabel = @"Base agent";
    addBaseAgents(_agent);
    [_agent selectItemAtIndex:[_agent indexOfItemWithRepresentedObject:previous[@"agent"] ?: @"opencode"]];
    _command = [NSTextField textFieldWithString:previous[@"command"] ?: @""];
    _command.accessibilityLabel = @"Shell command";
    _command.placeholderString = @"Alias or executable with arguments";
    _shortcut = [NSTextField textFieldWithString:previous[@"shortcut"] ?: @""];
    _shortcut.accessibilityLabel = @"Command shortcut";
    _shortcut.placeholderString = @"Optional, e.g. Ctrl+Q";
    for (NSTextField *field in @[_name, _command, _shortcut]) {
        field.delegate = self;
        field.font = [NSFont monospacedSystemFontOfSize:NSFont.systemFontSize weight:NSFontWeightRegular];
        field.usesSingleLineMode = YES;
        field.automaticTextCompletionEnabled = NO;
        [field setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
    }
    _error = [NSTextField wrappingLabelWithString:@""];
    _error.accessibilityLabel = @"Command error";
    _error.textColor = NSColor.systemRedColor;
    _error.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
    NSGridView *grid = MTFormGrid(@[
        @[[NSTextField labelWithString:@"Name"], _name],
        @[[NSTextField labelWithString:@"Agent"], _agent],
        @[[NSTextField labelWithString:@"Shell command"], _command],
        @[[NSTextField labelWithString:@"Shortcut"], _shortcut],
    ]);
    grid.rowSpacing = 10;

    NSButton *cancel = [NSButton buttonWithTitle:@"Cancel" target:self action:@selector(cancel:)];
    cancel.keyEquivalent = @"\e";
    NSButton *save = [NSButton buttonWithTitle:@"Save" target:self action:@selector(save:)];
    save.keyEquivalent = @"\r";
    NSStackView *buttons = [NSStackView stackViewWithViews:@[cancel, save]];
    NSView *content = _sheet.contentView;
    for (NSView *item in @[grid, _error, buttons]) {
        item.translatesAutoresizingMaskIntoConstraints = NO;
        [content addSubview:item];
    }
    [NSLayoutConstraint activateConstraints:@[
        [grid.topAnchor constraintEqualToAnchor:content.topAnchor constant:16],
        [grid.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:20],
        [grid.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-20],
        [_error.topAnchor constraintEqualToAnchor:grid.bottomAnchor constant:5],
        [_error.leadingAnchor constraintEqualToAnchor:grid.leadingAnchor],
        [_error.trailingAnchor constraintEqualToAnchor:grid.trailingAnchor],
        [buttons.bottomAnchor constraintEqualToAnchor:content.bottomAnchor constant:-14],
        [buttons.trailingAnchor constraintEqualToAnchor:grid.trailingAnchor],
    ]];
    _sheet.initialFirstResponder = _name;
    [_view.window beginSheet:_sheet completionHandler:nil];
    [_sheet makeFirstResponder:_name];
}

- (void)controlTextDidBeginEditing:(NSNotification *)note {
    // Shell quoting, dashes and model identifiers must stay exactly as typed.
    NSTextView *editor = note.userInfo[@"NSFieldEditor"];
    editor.automaticQuoteSubstitutionEnabled = NO;
    editor.automaticDashSubstitutionEnabled = NO;
    editor.automaticTextReplacementEnabled = NO;
    editor.automaticSpellingCorrectionEnabled = NO;
}

- (BOOL)cancelEditor {
    if (!_sheet) return NO;
    [self cancel:nil];
    return YES;
}

- (void)cancel:(id)sender {
    [_view.window endSheet:_sheet];
    _sheet = nil;
    _previous = nil;
}

- (void)save:(id)sender {
    NSCharacterSet *spaces = NSCharacterSet.whitespaceAndNewlineCharacterSet;
    NSDictionary *profile = @{@"name": [_name.stringValue stringByTrimmingCharactersInSet:spaces],
                              @"agent": _agent.selectedItem.representedObject,
                              @"shortcut": [_shortcut.stringValue stringByTrimmingCharactersInSet:spaces],
                              @"command": [_command.stringValue stringByTrimmingCharactersInSet:spaces]};
    NSMutableDictionary *edit = [@{@"replacement": profile} mutableCopy];
    if (_previous) edit[@"previous"] = _previous;
    NSString *error = [self edit:edit];
    if (error) {
        _error.stringValue = error;
        NSAccessibilityPostNotification(_error, NSAccessibilityValueChangedNotification);
        return;
    }
    NSInteger row = [[_commands valueForKey:@"name"] indexOfObject:profile[@"name"]];
    if (row != NSNotFound) [_table selectRowIndexes:[NSIndexSet indexSetWithIndex:row] byExtendingSelection:NO];
    [self cancel:sender];
}
@end
