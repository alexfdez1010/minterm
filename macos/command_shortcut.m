#import "command_shortcut.h"

static const NSEventModifierFlags shortcutModifiers = NSEventModifierFlagCommand | NSEventModifierFlagControl |
    NSEventModifierFlagOption | NSEventModifierFlagShift;

@implementation MTCommandShortcutButton {
    id _monitor;
    NSInteger _heldReturn;
    NSMutableIndexSet *_awaitingRelease;
}

- (instancetype)initWithShortcut:(NSString *)shortcut {
    if (!(self = [super initWithFrame:NSZeroRect])) return nil;
    _shortcut = [shortcut copy] ?: @"";
    self.bezelStyle = NSBezelStyleRounded;
    self.target = self;
    self.action = @selector(toggleRecording:);
    self.accessibilityLabel = @"Command shortcut";
    self.toolTip = @"Hold Control or Return and press a letter to record. Delete clears the shortcut.";
    self.title = _shortcut.length ? _shortcut : @"Record Shortcut…";
    return self;
}

- (BOOL)acceptsFirstResponder { return YES; }
- (BOOL)canBecomeKeyView { return self.enabled && !self.hiddenOrHasHiddenAncestor; }
- (id)accessibilityValue { return self.shortcut; }

- (void)keyDown:(NSEvent *)event {
    if (!event.isARepeat && !(event.modifierFlags & shortcutModifiers)) {
        if ([event.characters isEqual:@" "]) { [self performClick:nil]; return; }
        if (event.keyCode == 51 || event.keyCode == 117) { [self clearShortcut:self]; return; }
    }
    [super keyDown:event];
}

- (void)updateFeedback:(NSString *)error {
    self.title = _recording ? @"Press Ctrl + a letter" : (_shortcut.length ? _shortcut : @"Record Shortcut…");
    self.accessibilityHelp = _recording ? @"Hold Control or Return and press a letter. Escape cancels recording." : self.toolTip;
    if (self.feedback) self.feedback(error);
    if (_recording) {
        NSAccessibilityPostNotificationWithUserInfo(self, NSAccessibilityAnnouncementRequestedNotification,
            @{NSAccessibilityAnnouncementKey: error.length ? error : self.accessibilityHelp,
              NSAccessibilityPriorityKey: @(NSAccessibilityPriorityMedium)});
    }
}

- (void)removeMonitor {
    if (!_monitor) return;
    [NSEvent removeMonitor:_monitor];
    [NSNotificationCenter.defaultCenter removeObserver:self name:NSWindowDidResignKeyNotification object:nil];
    _monitor = nil;
    _awaitingRelease = nil;
    _heldReturn = -1;
}

- (void)stopRecording {
    [self removeMonitor];
    if (!_recording) return;
    _recording = NO;
    [self updateFeedback:@""];
}

- (void)clearShortcut:(id)sender {
    [self removeMonitor];
    _recording = NO;
    _shortcut = @"";
    [self updateFeedback:@""];
    [self.window makeFirstResponder:self];
    NSAccessibilityPostNotification(self, NSAccessibilityValueChangedNotification);
}

- (BOOL)resignFirstResponder {
    [self stopRecording];
    return [super resignFirstResponder];
}

- (void)windowResigned:(NSNotification *)notification { [self stopRecording]; }
- (void)dealloc { [self removeMonitor]; }

- (void)toggleRecording:(id)sender {
    if (_recording) { [self stopRecording]; return; }
    [self removeMonitor];
    if (![self.window makeFirstResponder:self]) return;
    _heldReturn = -1;
    _recording = YES;
    [self updateFeedback:@""];
    [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(windowResigned:)
        name:NSWindowDidResignKeyNotification object:self.window];
    __weak MTCommandShortcutButton *weakSelf = self;
    _monitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyDown | NSEventMaskKeyUp
        handler:^NSEvent *(NSEvent *event) {
            MTCommandShortcutButton *button = weakSelf;
            if (!button || event.window != button.window || button.window.firstResponder != button) return event;
            return [button capture:event];
        }];
}

- (NSEvent *)capture:(NSEvent *)event {
    NSUInteger code = event.keyCode;
    if (_awaitingRelease) {
        // A held Return can repeat after capture and activate the sheet's Save button.
        if (![_awaitingRelease containsIndex:code]) return event;
        if (event.type == NSEventTypeKeyUp || !event.isARepeat) {
            [_awaitingRelease removeIndex:code];
            if (!_awaitingRelease.count) [self removeMonitor];
        }
        return event.type == NSEventTypeKeyDown && !event.isARepeat ? event : nil;
    }
    if (event.type == NSEventTypeKeyUp) {
        if ((NSInteger)code != _heldReturn) return event;
        _heldReturn = -1;
        return nil;
    }
    if (event.isARepeat) return nil;
    NSEventModifierFlags flags = event.modifierFlags & shortcutModifiers;
    if (code == 53 && !flags) { [self stopRecording]; return nil; }
    if ((code == 51 || code == 117) && !flags) { [self clearShortcut:self]; return nil; }
    if (code == 48 && !(flags & ~NSEventModifierFlagShift)) {
        [self stopRecording];
        if (flags) [self.window selectPreviousKeyView:self];
        else [self.window selectNextKeyView:self];
        return nil;
    }
    if ((code == 36 || code == 76) && !flags) { _heldReturn = code; return nil; }

    NSString *letter = event.charactersIgnoringModifiers.uppercaseString;
    BOOL control = flags == NSEventModifierFlagControl && _heldReturn < 0;
    BOOL enter = !flags && _heldReturn >= 0;
    if ((!control && !enter) || letter.length != 1 || [letter characterAtIndex:0] < 'A' || [letter characterAtIndex:0] > 'Z') {
        [self updateFeedback:@"Hold Control or Return and press a letter."];
        return nil;
    }
    NSString *candidate = [(control ? @"Ctrl+" : @"Enter+") stringByAppendingString:letter];
    NSString *error = self.conflict ? self.conflict(candidate) : nil;
    if (error) { [self updateFeedback:error]; return nil; }

    _awaitingRelease = [NSMutableIndexSet indexSetWithIndex:code];
    if (enter) [_awaitingRelease addIndex:_heldReturn];
    _shortcut = candidate;
    _recording = NO;
    [self updateFeedback:@""];
    NSAccessibilityPostNotification(self, NSAccessibilityValueChangedNotification);
    return nil;
}
@end
