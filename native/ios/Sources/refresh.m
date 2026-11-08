// Refresh-rate request for the "Uncap FPS" setting. wgpu's Metal backend cannot disable display sync on iOS
// (CAMetalLayer.displaySyncEnabled is macOS-only), so the app asks for a frame-rate range instead.
// A display link whose callback does nothing is enough to make the system honour the range for the window.
// Needs `CADisableMinimumFrameDurationOnPhone` in Info.plist for rates above 60 Hz on iPhone (ProMotion).
#import <UIKit/UIKit.h>

@interface JMRefreshTarget : NSObject
- (void)tick:(CADisplayLink *)link;
@end
@implementation JMRefreshTarget
- (void)tick:(CADisplayLink *)link {}
@end

static CADisplayLink *jmLink;
static JMRefreshTarget *jmTarget;

// uncapped != 0: 80-120 Hz (prefers 120); otherwise 30-60 Hz (prefers 60, the default). Main thread only.
void jm_set_refresh_uncapped(int uncapped) {
    if (!jmLink) {
        jmTarget = [JMRefreshTarget new];
        jmLink = [CADisplayLink displayLinkWithTarget:jmTarget selector:@selector(tick:)];
        [jmLink addToRunLoop:NSRunLoop.mainRunLoop forMode:NSRunLoopCommonModes];
    }
    if (@available(iOS 15.0, *)) {
        jmLink.preferredFrameRateRange = uncapped ? CAFrameRateRangeMake(80, 120, 120) : CAFrameRateRangeMake(30, 60, 60);
    } else {
        jmLink.preferredFramesPerSecond = uncapped ? 120 : 60;
    }
}
