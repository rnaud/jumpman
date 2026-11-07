// iOS entry point. The game is a Rust static library (`jumpman_game`); `main_rs` (exported from
// lib.rs) starts Bevy/winit, and winit then runs UIApplicationMain itself.
#import <UIKit/UIKit.h>
#import <objc/runtime.h>

extern void main_rs(void);

static NSHashTable<UIWindow *> *sceneLessWindows;

// Attach a window created before any scene existed, then re-show it (a window that was shown
// scene-less stays blank otherwise).
static void JMAttach(UIWindow *window, UIWindowScene *scene) {
    if (window.windowScene) return;
    window.windowScene = scene;
    window.hidden = NO;
    [window makeKeyAndVisible];
    [window setNeedsLayout];
    [window.rootViewController.view setNeedsLayout];
    [window.rootViewController.view setNeedsDisplay];
} // windows created before any scene connected

// iOS 26+ requires the UIScene life cycle, but winit 0.30 still creates its UIWindow with
// `initWithFrame:` — a window with no scene is never shown (black screen, no draw callbacks, so
// Bevy gets stuck after its first update). Route those windows into the connected window scene.
@implementation UIWindow (JumpmanScene)
- (instancetype)jm_initWithFrame:(CGRect)frame {
    static __thread BOOL inScene = NO;
    if (!inScene) {
        for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
            if ([scene isKindOfClass:UIWindowScene.class]) {
                inScene = YES;
                self = [self initWithWindowScene:(UIWindowScene *)scene];
                inScene = NO;
                self.frame = frame;
                return self;
            }
        }
    }
    // No scene yet (the window can be created before UIKit connects it): make it, attach it later.
    UIWindow *window = [self jm_initWithFrame:frame]; // the original, after the swap
    if (window) [sceneLessWindows addObject:window];
    return window;
}
@end

int main(int argc, char *argv[]) {
    @autoreleasepool {
        sceneLessWindows = [NSHashTable weakObjectsHashTable];
        [[NSNotificationCenter defaultCenter] addObserverForName:UISceneWillConnectNotification object:nil
                                                           queue:NSOperationQueue.mainQueue
                                                      usingBlock:^(NSNotification *note) {
            if (![note.object isKindOfClass:UIWindowScene.class]) return;
            for (UIWindow *window in sceneLessWindows.allObjects) {
                JMAttach(window, (UIWindowScene *)note.object);
            }
        }];
        // Belt and braces: the connect notification can be missed, so also poll for a few seconds.
        __block int ticks = 0;
        [NSTimer scheduledTimerWithTimeInterval:0.1 repeats:YES block:^(NSTimer *timer) {
            UIWindowScene *scene = nil;
            for (UIScene *candidate in UIApplication.sharedApplication.connectedScenes) {
                if ([candidate isKindOfClass:UIWindowScene.class]) { scene = (UIWindowScene *)candidate; break; }
            }
            if (scene) {
                for (UIWindow *window in sceneLessWindows.allObjects) {
                    JMAttach(window, scene);
                }
            }
            if (++ticks > 100) [timer invalidate];
        }];
        method_exchangeImplementations(class_getInstanceMethod(UIWindow.class, @selector(initWithFrame:)),
                                       class_getInstanceMethod(UIWindow.class, @selector(jm_initWithFrame:)));
        main_rs();
    }
    return 0;
}

// Safe-area insets of the game window, in points: left, right, bottom, top. Main thread only.
void jm_safe_insets(float *out) {
    UIWindow *found = nil;
    for (UIScene *scene in UIApplication.sharedApplication.connectedScenes) {
        if (![scene isKindOfClass:UIWindowScene.class]) continue;
        for (UIWindow *window in ((UIWindowScene *)scene).windows) {
            if (!found || window.isKeyWindow) found = window;
        }
    }
    UIEdgeInsets i = found ? found.safeAreaInsets : UIEdgeInsetsZero;
    out[0] = (float)i.left;
    out[1] = (float)i.right;
    out[2] = (float)i.bottom;
    out[3] = (float)i.top;
}
