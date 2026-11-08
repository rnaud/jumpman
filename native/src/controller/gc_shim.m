// GameController.framework bridge shared by iOS and macOS. Handlers run on the main queue and push
// plain-C events into a locked ring; Rust drains it once per frame (`jm_pad_poll`) and turns them into
// Bevy `Gamepad` entities (see mod.rs / map.rs). Element ids here must match `map::Element`.
#import <Foundation/Foundation.h>
#import <GameController/GameController.h>
#include <pthread.h>
#include <string.h>

typedef struct { int32_t kind; int32_t uid; int32_t id; float value; } JMPadEvent;
enum { JMConnect = 0, JMDisconnect = 1, JMButton = 2, JMAxis = 3 };
#define QCAP 4096
#define NVALUES 21

static JMPadEvent gQueue[QCAP];
static int gHead, gTail;
static pthread_mutex_t gLock = PTHREAD_MUTEX_INITIALIZER;

static void push(int kind, int uid, int id, float value) {
    pthread_mutex_lock(&gLock);
    int next = (gTail + 1) % QCAP;
    if (next != gHead) { // drop on overflow rather than block the main queue
        gQueue[gTail] = (JMPadEvent){kind, uid, id, value};
        gTail = next;
    }
    pthread_mutex_unlock(&gLock);
}

@interface JMPad : NSObject
@property(strong) GCController *controller;
@property int uid;
@property float *last;
@end
@implementation JMPad
- (instancetype)init { if ((self = [super init])) _last = calloc(NVALUES, sizeof(float)); return self; }
- (void)dealloc { free(_last); }
@end

static NSMutableArray<JMPad *> *gPads;
static int gNextUid = 1;

static void sample(JMPad *p, GCExtendedGamepad *g) {
    float v[NVALUES] = {
        g.buttonA.value, g.buttonB.value, g.buttonX.value, g.buttonY.value,
        g.leftShoulder.value, g.rightShoulder.value, g.leftTrigger.value, g.rightTrigger.value,
        g.dpad.up.value, g.dpad.down.value, g.dpad.left.value, g.dpad.right.value,
        g.buttonMenu.value, g.buttonOptions.value, g.buttonHome.value,
        g.leftThumbstickButton.value, g.rightThumbstickButton.value,
        g.leftThumbstick.xAxis.value, g.leftThumbstick.yAxis.value,
        g.rightThumbstick.xAxis.value, g.rightThumbstick.yAxis.value,
    };
    for (int i = 0; i < NVALUES; i++) {
        if (fabsf(v[i] - p.last[i]) < 0.002f) continue;
        p.last[i] = v[i];
        if (i < 17) push(JMButton, p.uid, i, v[i]); else push(JMAxis, p.uid, i - 17, v[i]);
    }
}

static void attach(GCController *c) {
    if (!c.extendedGamepad) return; // Siri Remote / micro gamepads are not game controllers here
    for (JMPad *p in gPads) if (p.controller == c) return;
    JMPad *p = [JMPad new];
    p.controller = c;
    p.uid = gNextUid++;
    [gPads addObject:p];
    push(JMConnect, p.uid, 0, 0);
    __weak JMPad *weak = p;
    c.extendedGamepad.valueChangedHandler = ^(GCExtendedGamepad *g, GCControllerElement *e) {
        JMPad *strong = weak;
        if (strong) sample(strong, g);
    };
}

static void detach(GCController *c) {
    for (JMPad *p in [gPads copy]) {
        if (p.controller != c) continue;
        push(JMDisconnect, p.uid, 0, 0);
        [gPads removeObject:p];
    }
}

void jm_pad_start(void) {
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        gPads = [NSMutableArray array];
#if TARGET_OS_OSX
        // Keep reading the pad while the window is not frontmost.
        GCController.shouldMonitorBackgroundEvents = YES;
#endif
        NSNotificationCenter *nc = NSNotificationCenter.defaultCenter;
        [nc addObserverForName:GCControllerDidConnectNotification object:nil queue:NSOperationQueue.mainQueue
                    usingBlock:^(NSNotification *n) { attach(n.object); }];
        [nc addObserverForName:GCControllerDidDisconnectNotification object:nil queue:NSOperationQueue.mainQueue
                    usingBlock:^(NSNotification *n) { detach(n.object); }];
        for (GCController *c in GCController.controllers) attach(c);
    });
}

int jm_pad_poll(JMPadEvent *out, int max) {
    int n = 0;
    pthread_mutex_lock(&gLock);
    while (gHead != gTail && n < max) {
        out[n++] = gQueue[gHead];
        gHead = (gHead + 1) % QCAP;
    }
    pthread_mutex_unlock(&gLock);
    return n;
}

// UTF-8 product name (e.g. "DualSense Wireless Controller"); returns the length written.
int jm_pad_name(int uid, char *buf, int len) {
    __block int n = 0;
    void (^work)(void) = ^{
        for (JMPad *p in gPads) {
            if (p.uid != uid) continue;
            NSString *s = p.controller.vendorName ?: @"Controller";
            strlcpy(buf, s.UTF8String, len);
            n = (int)strlen(buf);
        }
    };
    if (NSThread.isMainThread) work(); else dispatch_sync(dispatch_get_main_queue(), work);
    return n;
}

// Light the player LED (Xbox ring, DualSense bar) for the assigned slot.
void jm_pad_set_player(int uid, int slot) {
    void (^work)(void) = ^{
        for (JMPad *p in gPads) {
            if (p.uid != uid) continue;
            p.controller.playerIndex = slot >= 0 && slot < 4 ? (GCControllerPlayerIndex)slot : GCControllerPlayerIndexUnset;
        }
    };
    if (NSThread.isMainThread) work(); else dispatch_async(dispatch_get_main_queue(), work);
}
