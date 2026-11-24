#import <AppKit/AppKit.h>

// Sparkle is loaded from the app bundle so cargo runs and native tests do not
// need a separately installed framework.
@interface MTUpdater : NSObject
@property(nonatomic) BOOL automaticallyChecksForUpdates;
@property(nonatomic) BOOL automaticallyDownloadsUpdates;
- (void)checkForUpdatesInBackground;
@end
@interface MTUpdaterController : NSObject
@property(nonatomic, readonly) MTUpdater *updater;
- (instancetype)initWithStartingUpdater:(BOOL)start updaterDelegate:(id)delegate userDriverDelegate:(id)driver;
- (void)startUpdater;
@end

static MTUpdaterController *controller;

void minterm_set_automatic_updates(bool enabled, bool checkNow) {
    if (!controller && !enabled) return;
    if (!controller) {
        NSString *path = [NSBundle.mainBundle.privateFrameworksPath stringByAppendingPathComponent:@"Sparkle.framework"];
        NSBundle *framework = [NSBundle bundleWithPath:path];
        if (![framework load]) { NSLog(@"Minterm: Sparkle framework unavailable"); return; }
        Class updaterClass = NSClassFromString(@"SPUStandardUpdaterController");
        controller = [[updaterClass alloc] initWithStartingUpdater:NO updaterDelegate:nil userDriverDelegate:nil];
        controller.updater.automaticallyChecksForUpdates = YES;
        controller.updater.automaticallyDownloadsUpdates = YES;
        [controller startUpdater];
        if (checkNow) [controller.updater checkForUpdatesInBackground];
        return;
    }
    controller.updater.automaticallyDownloadsUpdates = enabled;
    controller.updater.automaticallyChecksForUpdates = enabled;
}
