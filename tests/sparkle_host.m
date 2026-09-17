#import <AppKit/AppKit.h>
#import <Sparkle/Sparkle.h>

// A disposable host for exercising Sparkle's real download/validation/installer
// pipeline without registering Winlane shortcuts or touching Accessibility.
@interface TestDriver : NSObject <SPUUserDriver>
@end
@implementation TestDriver
- (void)showUpdatePermissionRequest:(SPUUpdatePermissionRequest *)request reply:(void (^)(SUUpdatePermissionResponse *))reply {
    reply([[SUUpdatePermissionResponse alloc] initWithAutomaticUpdateChecks:NO sendSystemProfile:NO]);
}
- (void)showUserInitiatedUpdateCheckWithCancellation:(void (^)(void))cancellation {}
- (void)showUpdateFoundWithAppcastItem:(SUAppcastItem *)item state:(SPUUserUpdateState *)state reply:(void (^)(SPUUserUpdateChoice))reply {
    reply(SPUUserUpdateChoiceInstall);
}
- (void)showUpdateReleaseNotesWithDownloadData:(SPUDownloadData *)data {}
- (void)showUpdateReleaseNotesFailedToDownloadWithError:(NSError *)error {}
- (void)showUpdateNotFoundWithError:(NSError *)error acknowledgement:(void (^)(void))reply {
    NSLog(@"Unexpected missing update: %@", error); exit(2);
}
- (void)showUpdaterError:(NSError *)error acknowledgement:(void (^)(void))reply {
    NSLog(@"Updater error: %@", error);
    NSNumber *expected = [[NSBundle mainBundle] objectForInfoDictionaryKey:@"TestExpectedError"];
    BOOL validationFailed = NO;
    for (NSError *cause = error; cause != nil; cause = cause.userInfo[NSUnderlyingErrorKey]) {
        if ([cause.domain isEqualToString:SUSparkleErrorDomain] && cause.code == SUValidationError) validationFailed = YES;
    }
    exit(expected && validationFailed && [error.domain isEqualToString:SUSparkleErrorDomain]
        && error.code == expected.integerValue ? 0 : 3);
}
- (void)showDownloadInitiatedWithCancellation:(void (^)(void))cancellation {}
- (void)showDownloadDidReceiveExpectedContentLength:(uint64_t)length {}
- (void)showDownloadDidReceiveDataOfLength:(uint64_t)length {}
- (void)showDownloadDidStartExtractingUpdate {}
- (void)showExtractionReceivedProgress:(double)progress {}
- (void)showReadyToInstallAndRelaunch:(void (^)(SPUUserUpdateChoice))reply { reply(SPUUserUpdateChoiceInstall); }
- (void)showInstallingUpdateWithApplicationTerminated:(BOOL)terminated retryTerminatingApplication:(void (^)(void))retry {}
- (void)showUpdateInstalledAndRelaunched:(BOOL)relaunched acknowledgement:(void (^)(void))reply { reply(); }
- (void)dismissUpdateInstallation {}
@end

int main(void) {
    @autoreleasepool {
        NSBundle *bundle = [NSBundle mainBundle];
        if ([[bundle objectForInfoDictionaryKey:@"CFBundleVersion"] isEqualToString:@"1.0.1"]) {
            NSError *error = nil;
            BOOL written = [@"updated and relaunched" writeToFile:[bundle objectForInfoDictionaryKey:@"TestMarker"]
                atomically:YES encoding:NSUTF8StringEncoding error:&error];
            return written ? 0 : 4;
        }
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyProhibited];
        TestDriver *driver = [TestDriver new];
        SPUUpdater *updater = [[SPUUpdater alloc] initWithHostBundle:bundle applicationBundle:bundle userDriver:driver delegate:nil];
        NSError *error = nil;
        if (![updater startUpdater:&error]) { NSLog(@"Start failed: %@", error); return 5; }
        [updater checkForUpdates];
        dispatch_after(dispatch_time(DISPATCH_TIME_NOW, 45 * NSEC_PER_SEC), dispatch_get_main_queue(), ^{ exit(6); });
        [NSApp run];
    }
    return 0;
}
