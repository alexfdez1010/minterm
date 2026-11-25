#import "preferences.h"

// The caller synchronously returns updated preferences to minterm_settings_reload.
extern void minterm_refresh_settings(void);

static NSSize paneSize(NSString *identifier) {
    return [identifier isEqual:@"Commands"] ? NSMakeSize(812, 320) : NSMakeSize(480, 250);
}

@interface MTSettingsController : NSObject <NSWindowDelegate, NSTabViewDelegate>
@property(nonatomic, strong) NSWindow *window;
@property(nonatomic, strong) MTPreferencesPane *preferences;
@property(nonatomic, strong) NSTabView *tabs;
@end

@implementation MTSettingsController
- (instancetype)initWithChange:(MTChangePreference)change {
    if (!(self = [super init])) return nil;
    NSRect initial = {NSZeroPoint, paneSize(@"General")};
    _window = [[NSWindow alloc] initWithContentRect:initial
        styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable
        backing:NSBackingStoreBuffered defer:NO];
    _window.title = @"Minterm Settings";
    _window.releasedWhenClosed = NO;
    _window.restorable = NO;
    _window.delegate = self;

    _preferences = [[MTPreferencesPane alloc] initWithChange:change];
    _tabs = [[NSTabView alloc] initWithFrame:NSInsetRect(_window.contentView.bounds, 12, 12)];
    _tabs.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    for (NSArray *entry in @[@[@"General", _preferences.generalView], @[@"Commands", _preferences.commandsView]]) {
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

- (void)positionPane:(NSString *)identifier relativeTo:(NSWindow *)anchor {
    NSScreen *screen = anchor.screen ?: NSScreen.mainScreen;
    NSRect visible = screen.visibleFrame;
    NSRect content = {NSZeroPoint, paneSize(identifier)};
    NSRect frame = [self.window frameRectForContentRect:content];
    frame.size.width = MIN(NSWidth(frame), NSWidth(visible));
    frame.size.height = MIN(NSHeight(frame), NSHeight(visible));
    frame.origin.x = MIN(MAX(round(NSMidX(anchor.frame) - NSWidth(frame) / 2), NSMinX(visible)),
                         NSMaxX(visible) - NSWidth(frame));
    frame.origin.y = MIN(MAX(round(NSMidY(anchor.frame) - NSHeight(frame) / 2), NSMinY(visible)),
                         NSMaxY(visible) - NSHeight(frame));
    [self.window setFrame:frame display:self.window.isVisible];
}

- (void)tabView:(NSTabView *)tabView didSelectTabViewItem:(NSTabViewItem *)item {
    [self.preferences stopRecording];
    if (self.window.isVisible) [self positionPane:item.identifier relativeTo:self.window];
}

- (void)show:(NSDictionary *)prefs forTerminal:(NSWindow *)terminal {
    if (!self.window.isVisible)
        [self positionPane:self.tabs.selectedTabViewItem.identifier relativeTo:terminal];
    [self.preferences reload:prefs];
    [self.window makeKeyAndOrderFront:nil];
}
@end

static MTSettingsController *controller;

void minterm_settings_show(NSWindow *terminal, const char *prefs, MTChangePreference change) {
    if (!controller) controller = [[MTSettingsController alloc] initWithChange:change];
    NSDictionary *values = [NSJSONSerialization JSONObjectWithData:[@(prefs) dataUsingEncoding:NSUTF8StringEncoding] options:0 error:NULL];
    [controller show:values forTerminal:terminal];
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
