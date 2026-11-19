// Test-only AppKit adapter. Events are confined to this process's hidden window.
#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#import "preferences_driver.m"

@interface MintermTestKeys : NSObject <WKScriptMessageHandlerWithReply>
@property(nonatomic, weak) NSWindow *window;
@property(nonatomic, weak) WKWebView *view;
@end

@implementation MintermTestKeys
- (void)userContentController:(WKUserContentController *)controller
      didReceiveScriptMessage:(WKScriptMessage *)message
                 replyHandler:(void (^)(id, NSString *))reply {
    NSDictionary *key = message.body;
    if (key[@"preferences"]) { qa_preferences(key[@"preferences"], reply); return; }
    if (key[@"menuItem"]) {
        NSMenu *menu = [NSApp.mainMenu itemWithTitle:@"Shell"].submenu;
        NSInteger index = [menu indexOfItemWithTitle:key[@"menuItem"]];
        if (!menu || index < 0) { reply(nil, @"Shell menu item unavailable"); return; }
        [menu performActionForItemAtIndex:index];
        reply(@YES, nil);
        return;
    }
    NSUInteger modifiers = [key[@"modifiers"] unsignedIntegerValue];
    [self.window makeFirstResponder:self.view];
    NSEvent *event = [NSEvent keyEventWithType:NSEventTypeKeyDown
        location:NSZeroPoint modifierFlags:modifiers
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:self.window.windowNumber
        context:nil characters:key[@"text"] charactersIgnoringModifiers:key[@"plain"] ?: key[@"text"]
        isARepeat:NO keyCode:[key[@"code"] unsignedShortValue]];
    BOOL handled = (modifiers & NSEventModifierFlagCommand) && [NSApp.mainMenu performKeyEquivalent:event];
    if (!handled) [self.window sendEvent:event];
    reply(@(handled), nil);
}
@end

static WKWebView *findView(NSView *root) {
    if ([root isKindOfClass:WKWebView.class]) return (WKWebView *)root;
    for (NSView *child in root.subviews) {
        WKWebView *view = findView(child);
        if (view) return view;
    }
    return nil;
}

void qa_install(void *windowPointer, const char *source) {
    NSWindow *window = (__bridge NSWindow *)windowPointer;
    WKWebView *view = findView(window.contentView);
    if (!view) { fprintf(stderr, "Missing terminal WKWebView\n"); exit(1); }
    if (@available(macOS 14.0, *)) view.configuration.preferences.inactiveSchedulingPolicy = WKInactiveSchedulingPolicyNone;
    MintermTestKeys *driver = [MintermTestKeys new];
    driver.window = window;
    driver.view = view;
    [view.configuration.userContentController addScriptMessageHandlerWithReply:driver
        contentWorld:WKContentWorld.pageWorld name:@"testKey"];
    NSString *script = @(source);
    // Wait for the real local navigation before starting the DOM assertions.
    [NSTimer scheduledTimerWithTimeInterval:0.05 repeats:YES block:^(NSTimer *timer) {
        if (view.loading || ![view.URL.scheme isEqual:@"minterm"]) return;
        [timer invalidate];
        [view callAsyncJavaScript:script arguments:@{} inFrame:nil inContentWorld:WKContentWorld.pageWorld
            completionHandler:^(id value, NSError *error) {
                if (error || ![value isEqual:@"passed"]) {
                    fprintf(stderr, "Terminal smoke test failed: %s\n",
                        (error ? error.userInfo.description : [value description]).UTF8String);
                } else {
                    NSString *home = NSProcessInfo.processInfo.environment[@"HOME"];
                    [NSData.data writeToFile:[home stringByAppendingPathComponent:@"smoke-passed"] atomically:YES];
                }
                [view evaluateJavaScript:@"window.ipc.postMessage(JSON.stringify({action:'quit',force:true}))" completionHandler:nil];
            }];
    }];
}
