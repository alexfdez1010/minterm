#import <AppKit/AppKit.h>

NSGridView *MTFormGrid(NSArray<NSArray<NSView *> *> *rows);

typedef bool (*MTChangePreference)(const char *, const char *, char *, size_t);

@interface MTPreferencesPane : NSObject
@property(nonatomic, readonly) NSView *generalView;
@property(nonatomic, readonly) NSView *commandsView;
- (instancetype)initWithChange:(MTChangePreference)change;
- (void)reload:(NSDictionary *)values;
- (void)reloadCommands:(NSDictionary *)values;
- (BOOL)cancelCommandEditor;
- (void)stopRecording;
@end
