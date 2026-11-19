#import "preferences.h"

// The caller synchronously returns updated preferences to minterm_settings_reload.
extern void minterm_refresh_settings(void);

@interface MTSettingsController : NSObject <NSWindowDelegate, NSTabViewDelegate>
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) MTPreferencesPane *preferences;
@property(nonatomic, strong) NSTabView *tabs;
@end

@implementation MTSettingsController
- (instancetype)initWithChange:(MTChangePreference)change {
    if (!(self = [super init])) return nil;
    _window = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 540, 276)
        styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
        backing:NSBackingStoreBuffered defer:NO];
    _window.title = @"Minterm Settings";
    _window.releasedWhenClosed = NO;
    _window.restorable = NO;
    _window.delegate = self;
    [_window center];

    _preferences = [[MTPreferencesPane alloc] initWithChange:change];
    _tabs = [[NSTabView alloc] initWithFrame:NSInsetRect(_window.contentView.bounds, 12, 12)];
    _tabs.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    for (NSArray *entry in @[@[@"General", _preferences.generalView], @[@"Commands", _preferences.commandsView], @[@"Codex", _preferences.codexView]]) {
        NSTabViewItem *item = [[NSTabViewItem alloc] initWithIdentifier:entry[0]];
        item.label = entry[0];
        item.view = entry[1];
        [_tabs addTabViewItem:item];
    }
    _tabs.delegate = self;
    [_window.contentView addSubview:_tabs];
    return self;
}

- (void)windowWillClose:(NSNotification *)notification {
    [self.window makeFirstResponder:nil];
    [self.preferences stopRecording];
}

- (void)windowDidBecomeKey:(NSNotification *)notification { minterm_refresh_settings(); }

- (void)windowDidResignKey:(NSNotification *)notification {
    [self.preferences stopRecording];
}

- (void)tabView:(NSTabView *)tabView didSelectTabViewItem:(NSTabViewItem *)item {
    [self.preferences stopRecording];
    if ([item.identifier isEqual:@"Codex"]) [self.preferences refreshModels];
}

- (void)show:(NSDictionary *)prefs {
    [self.preferences reload:prefs];
    [self.window makeKeyAndOrderFront:nil];
}
@end

static MTSettingsController *controller;

void minterm_settings_show(const char *prefs, MTChangePreference change) {
    if (!controller) controller = [[MTSettingsController alloc] initWithChange:change];
    NSDictionary *values = [NSJSONSerialization JSONObjectWithData:[@(prefs) dataUsingEncoding:NSUTF8StringEncoding] options:0 error:NULL];
    [controller show:values];
}

bool minterm_settings_close_if_key(void) {
    NSWindow *sheet = controller.window.attachedSheet;
    if (sheet && (sheet.isKeyWindow || controller.window.isKeyWindow)) {
        if (![controller.preferences cancelCommandEditor]) [sheet performClose:nil];
        return true;
    }
    if (!controller.window.isKeyWindow) return false;
    [controller.window performClose:nil];
    return true;
}

void minterm_settings_reload(const char *prefs) {
    NSDictionary *values = [NSJSONSerialization JSONObjectWithData:[@(prefs) dataUsingEncoding:NSUTF8StringEncoding] options:0 error:NULL];
    if (values) [controller.preferences reloadCommands:values];
}

void minterm_reveal_agent_commands(const char *path) {
    NSURL *file = [NSURL fileURLWithPath:@(path)];
    [NSWorkspace.sharedWorkspace activateFileViewerSelectingURLs:@[file]];
}
