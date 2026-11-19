#import <AppKit/AppKit.h>

void MTAddBuiltInAgents(NSPopUpButton *popup);
NSString *MTAgentTitle(NSString *agent);

@interface MTAgentCommands : NSObject <NSTableViewDataSource, NSTableViewDelegate, NSTextFieldDelegate>
@property(nonatomic, readonly) NSView *view;
- (instancetype)initWithSave:(NSString *(^)(NSDictionary *edit))save reveal:(NSString *(^)(void))reveal;
- (void)reload:(NSArray<NSDictionary *> *)commands error:(NSString *)error;
- (BOOL)cancelEditor;
@end
