#import <AppKit/AppKit.h>

typedef bool (*MTChangePreference)(const char *, const char *, char *, size_t);

@interface MTPreferencesPane : NSObject <NSComboBoxDelegate, NSTextFieldDelegate>
@property(nonatomic, readonly) NSView *generalView;
@property(nonatomic, readonly) NSView *ollamaView;
- (instancetype)initWithChange:(MTChangePreference)change;
- (void)reload:(NSDictionary *)values;
- (void)stopRecording;
- (void)refreshModels;
@end
