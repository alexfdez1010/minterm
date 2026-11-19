#import <AppKit/AppKit.h>

@interface MTCommandShortcutButton : NSButton
@property(nonatomic, copy, readonly) NSString *shortcut;
@property(nonatomic, readonly) BOOL recording;
@property(nonatomic, copy) NSString *(^conflict)(NSString *shortcut);
@property(nonatomic, copy) void (^feedback)(NSString *error);
- (instancetype)initWithShortcut:(NSString *)shortcut;
- (void)clearShortcut:(id)sender;
- (void)stopRecording;
@end
