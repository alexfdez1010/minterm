#import <AppKit/AppKit.h>
#import "preferences.h"

typedef int (*MTUpdate)(int action, int provider, const unsigned char *key,
                        size_t length, char *message, size_t capacity);

typedef NS_ENUM(int, MTKeyAction) { MTReadStatus, MTSaveKey, MTRemoveKey };

@interface MTSettingsController : NSObject <NSWindowDelegate, NSTextFieldDelegate, NSTabViewDelegate>
@property (nonatomic, strong) NSWindow *window;
@property (nonatomic, strong) NSPopUpButton *providers;
@property (nonatomic, strong) NSSecureTextField *field;
@property (nonatomic, strong) NSButton *saveButton;
@property (nonatomic, strong) NSButton *removeButton;
@property (nonatomic, strong) NSTextField *status;
@property (nonatomic, strong) dispatch_queue_t queue;
@property (nonatomic) MTUpdate update;
@property (nonatomic) NSUInteger generation;
@property (nonatomic) BOOL busy;
@property (nonatomic) BOOL saved;
@property (nonatomic, strong) MTPreferencesPane *preferences;
@property (nonatomic, strong) NSTabView *tabs;
@end

@implementation MTSettingsController
- (instancetype)initWithUpdate:(MTUpdate)update change:(MTChangePreference)change {
    self = [super init];
    if (!self) return nil;
    _queue = dispatch_queue_create("com.minterm.settings", DISPATCH_QUEUE_SERIAL);
    _update = update;
    _window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 490, 276)
                                         styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
                                           backing:NSBackingStoreBuffered defer:NO];
    _window.title = @"Minterm Settings";
    _window.releasedWhenClosed = NO;
    _window.restorable = NO;
    _window.delegate = self;
    [_window center];

    NSTextField *providerLabel = [NSTextField labelWithString:@"Provider"];
    NSTextField *keyLabel = [NSTextField labelWithString:@"API key"];
    _providers = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    [_providers addItemsWithTitles:@[@"OpenAI", @"Anthropic"]];
    _providers.accessibilityLabel = @"Provider";
    _providers.target = self;
    _providers.action = @selector(providerChanged:);
    _field = [[NSSecureTextField alloc] initWithFrame:NSZeroRect];
    _field.accessibilityLabel = @"API key";
    _field.delegate = self;
    _field.target = self;
    _field.action = @selector(saveKey:);
    _status = [NSTextField wrappingLabelWithString:@""];
    _status.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
    _status.textColor = NSColor.secondaryLabelColor;
    _status.preferredMaxLayoutWidth = 318;
    _saveButton = [NSButton buttonWithTitle:@"Save Key" target:self action:@selector(saveKey:)];
    _saveButton.keyEquivalent = @"\r";
    _removeButton = [NSButton buttonWithTitle:@"Remove Key" target:self action:@selector(removeKey:)];
    _removeButton.toolTip = @"Remove this provider’s key from Minterm";
    _window.initialFirstResponder = _field;
    NSView *content = [NSView new];
    _preferences = [[MTPreferencesPane alloc] initWithChange:change];
    _tabs = [[NSTabView alloc] initWithFrame:NSInsetRect(_window.contentView.bounds, 12, 12)];
    _tabs.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    for (NSArray *entry in @[@[@"General", _preferences.generalView], @[@"API Keys", content], @[@"Ollama", _preferences.ollamaView]]) {
        NSTabViewItem *item = [[NSTabViewItem alloc] initWithIdentifier:entry[0]];
        item.label = entry[0];
        item.view = entry[1];
        [_tabs addTabViewItem:item];
    }
    _tabs.delegate = self;
    [_window.contentView addSubview:_tabs];
    for (NSView *view in @[providerLabel, keyLabel, _providers, _field, _status, _removeButton, _saveButton]) {
        view.translatesAutoresizingMaskIntoConstraints = NO;
        [content addSubview:view];
    }
    [NSLayoutConstraint activateConstraints:@[
        [_providers.topAnchor constraintEqualToAnchor:content.topAnchor constant:20],
        [_providers.leadingAnchor constraintEqualToAnchor:_field.leadingAnchor],
        [_providers.widthAnchor constraintEqualToConstant:160],
        [providerLabel.trailingAnchor constraintEqualToAnchor:_providers.leadingAnchor constant:-12],
        [providerLabel.centerYAnchor constraintEqualToAnchor:_providers.centerYAnchor],
        [_field.topAnchor constraintEqualToAnchor:_providers.bottomAnchor constant:12],
        [_field.widthAnchor constraintEqualToConstant:318],
        [_field.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-24],
        [keyLabel.trailingAnchor constraintEqualToAnchor:providerLabel.trailingAnchor],
        [keyLabel.centerYAnchor constraintEqualToAnchor:_field.centerYAnchor],
        [_status.leadingAnchor constraintEqualToAnchor:_field.leadingAnchor],
        [_status.trailingAnchor constraintEqualToAnchor:_field.trailingAnchor],
        [_status.topAnchor constraintEqualToAnchor:_field.bottomAnchor constant:6],
        [_status.bottomAnchor constraintLessThanOrEqualToAnchor:_saveButton.topAnchor constant:-10],
        [_saveButton.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-24],
        [_saveButton.bottomAnchor constraintEqualToAnchor:content.bottomAnchor constant:-20],
        [_removeButton.trailingAnchor constraintEqualToAnchor:_saveButton.leadingAnchor constant:-8],
        [_removeButton.centerYAnchor constraintEqualToAnchor:_saveButton.centerYAnchor],
    ]];
    _providers.nextKeyView = _field;
    _field.nextKeyView = _removeButton;
    _removeButton.nextKeyView = _saveButton;
    _saveButton.nextKeyView = _providers;
    return self;
}

- (void)clearField {
    self.field.stringValue = @"";
    self.field.currentEditor.string = @"";
}

- (void)refreshControls {
    self.saveButton.enabled = !self.busy && self.field.stringValue.length > 0;
    self.removeButton.enabled = !self.busy && self.saved;
}

- (void)controlTextDidChange:(NSNotification *)notification {
    [self refreshControls];
}

- (void)performAction:(MTKeyAction)action {
    NSUInteger generation = ++self.generation;
    int provider = (int)self.providers.indexOfSelectedItem;
    NSString *input = action == MTSaveKey ? self.field.stringValue : nil;
    [self clearField];
    self.busy = YES;
    self.providers.enabled = action == MTReadStatus;
    self.field.enabled = action == MTReadStatus;
    self.status.stringValue = @"";
    [self refreshControls];
    dispatch_async(self.queue, ^{
        char reply[1024] = {0};
        int result = self.update(action, provider, (const unsigned char *)input.UTF8String,
                                 [input lengthOfBytesUsingEncoding:NSUTF8StringEncoding], reply, sizeof(reply));
        NSString *message = [NSString stringWithUTF8String:reply];
        dispatch_async(dispatch_get_main_queue(), ^{
            if (self.generation != generation) return;
            self.busy = NO;
            if (result >= 0) self.saved = result == 1;
            self.status.stringValue = message;
            self.status.textColor = result < 0 ? NSColor.systemRedColor : NSColor.secondaryLabelColor;
            self.providers.enabled = YES;
            self.field.enabled = YES;
            [self refreshControls];
            if (action != MTReadStatus || result < 0) {
                NSAccessibilityPostNotificationWithUserInfo(self.status, NSAccessibilityAnnouncementRequestedNotification,
                    @{NSAccessibilityAnnouncementKey: message, NSAccessibilityPriorityKey: @(NSAccessibilityPriorityMedium)});
                [self.window makeFirstResponder:self.field];
            }
        });
    });
}

- (void)providerChanged:(id)sender {
    self.saved = NO;
    [self performAction:MTReadStatus];
}

- (void)saveKey:(id)sender {
    if (self.saveButton.enabled) [self performAction:MTSaveKey];
}

- (void)removeKey:(id)sender {
    if (self.removeButton.enabled) [self performAction:MTRemoveKey];
}

- (void)windowWillClose:(NSNotification *)notification {
    [self.window makeFirstResponder:nil];
    [self.preferences stopRecording];
    ++self.generation;
    [self clearField];
    self.status.stringValue = @"";
}

- (void)windowDidResignKey:(NSNotification *)notification {
    [self.preferences stopRecording];
}

- (void)tabView:(NSTabView *)tabView didSelectTabViewItem:(NSTabViewItem *)item {
    [self.preferences stopRecording];
    if ([item.identifier isEqual:@"Ollama"]) [self.preferences refreshModels];
    if ([item.identifier isEqual:@"API Keys"]) [self providerChanged:nil];
}

- (void)show:(NSDictionary *)prefs {
    [self.preferences reload:prefs];
    [self providerChanged:nil];
    [self.window makeKeyAndOrderFront:nil];
    if ([self.tabs.selectedTabViewItem.identifier isEqual:@"API Keys"]) [self.window makeFirstResponder:self.field];
}
@end

static MTSettingsController *controller;

void minterm_settings_show(MTUpdate update, const char *prefs, MTChangePreference change) {
    if (!controller) controller = [[MTSettingsController alloc] initWithUpdate:update change:change];
    NSDictionary *values = [NSJSONSerialization JSONObjectWithData:[@(prefs) dataUsingEncoding:NSUTF8StringEncoding] options:0 error:NULL];
    [controller show:values];
}

bool minterm_settings_close_if_key(void) {
    if (!controller.window.isKeyWindow) return false;
    [controller.window performClose:nil];
    return true;
}
