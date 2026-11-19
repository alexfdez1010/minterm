#import "preferences.h"

@interface MTAgentCommands : NSObject <NSTableViewDataSource, NSTableViewDelegate, NSTextFieldDelegate>
@property(nonatomic, readonly) NSView *view;
- (instancetype)initWithChange:(MTChangePreference)change;
- (void)reload:(NSArray<NSDictionary *> *)commands defaultName:(NSString *)name error:(NSString *)error;
- (BOOL)cancelEditor;
@end
