// iOS ROM import host (BYOR build only): the document picker / Files integration, the data dir and the keep-awake +
// background-task handling around a long extraction. The import itself is the pure-Rust byor-rs crate (src/byor.rs).
// Bridged to Rust (src/byor_ios.rs) with plain C functions, like the controller bridge (src/controller/gc_shim.m).
#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#include <objc/runtime.h>
#include <sys/resource.h>
#include <os/proc.h>

// ------------------------------------------------------------------------------------------------ paths

static NSString *JMDataDir(void) {
    NSString *env = NSProcessInfo.processInfo.environment[@"JUMPMAN_DATA_DIR"];
    if (env.length) return env;
    NSURL *support = [NSFileManager.defaultManager URLsForDirectory:NSApplicationSupportDirectory inDomains:NSUserDomainMask].firstObject;
    return [support.path stringByAppendingPathComponent:@"Jumpman"];
}

// ------------------------------------------------------------------------------------------------ lifecycle log

// One timestamped line in <data>/import.log (the Rust side appends to the same file), so a device log or a bug report shows what iOS
// did to the app during an import: backgrounded, suspended, memory warnings, background-task expiry. `avail` is the memory the
// system would still give this process (os_proc_available_memory), the number that decides a jetsam kill.
static void JMLog(NSString *msg) {
    static NSDateFormatter *fmt; static dispatch_once_t once;
    dispatch_once(&once, ^{ fmt = [NSDateFormatter new]; fmt.dateFormat = @"HH:mm:ss.SSS"; });
    NSString *line = [NSString stringWithFormat:@"[ios %@] %@ (avail %zu MB)\n", [fmt stringFromDate:NSDate.date], msg, os_proc_available_memory() >> 20];
    NSLog(@"jumpman: %@", msg);
    NSString *path = [JMDataDir() stringByAppendingPathComponent:@"import.log"];
    NSFileHandle *h = [NSFileHandle fileHandleForWritingAtPath:path];
    if (!h) { [NSFileManager.defaultManager createFileAtPath:path contents:nil attributes:nil]; h = [NSFileHandle fileHandleForWritingAtPath:path]; }
    @try { [h seekToEndOfFile]; [h writeData:[line dataUsingEncoding:NSUTF8StringEncoding]]; } @catch (NSException *e) {}
    [h closeFile];
}

static void JMObserveLifecycle(void) {
    NSNotificationCenter *nc = NSNotificationCenter.defaultCenter;
    NSArray *names = @[UIApplicationDidEnterBackgroundNotification, UIApplicationWillEnterForegroundNotification, UIApplicationDidBecomeActiveNotification,
                       UIApplicationWillResignActiveNotification, UIApplicationDidReceiveMemoryWarningNotification, UIApplicationWillTerminateNotification];
    for (NSString *n in names) {
        [nc addObserverForName:n object:nil queue:nil usingBlock:^(NSNotification *note) { JMLog(note.name); }];
    }
}

// Creates <Application Support>/Jumpman and keeps it out of iCloud / device backups (the extracted data is ~300 MB
// and can be regenerated from the ROMs).
const char *jm_prepare_data_dir(void) {
    static NSString *dir; static dispatch_once_t once;
    dispatch_once(&once, ^{
        dir = JMDataDir();
        [NSFileManager.defaultManager createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:nil];
        [[NSURL fileURLWithPath:dir] setResourceValue:@YES forKey:NSURLIsExcludedFromBackupKey error:nil];
        JMObserveLifecycle();
    });
    return dir.fileSystemRepresentation;
}

const char *jm_documents_dir(void) {
    static NSString *dir; static dispatch_once_t once;
    dispatch_once(&once, ^{ dir = NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject; });
    return dir.fileSystemRepresentation;
}

const char *jm_bundle_dir(void) { return NSBundle.mainBundle.bundlePath.fileSystemRepresentation; }

// ------------------------------------------------------------------------------------------------ keep awake

static UIBackgroundTaskIdentifier g_bg;  // set to UIBackgroundTaskInvalid at boot (not a constant expression)

// While an extraction runs: no auto-lock, and a background-task assertion so a short app switch finishes
// gracefully (the atomic stage means an interrupted import leaves the previous install intact).
void jm_keep_awake(int on) {
    dispatch_async(dispatch_get_main_queue(), ^{
        UIApplication.sharedApplication.idleTimerDisabled = on ? YES : NO;
        if (on && g_bg == UIBackgroundTaskInvalid) {
            g_bg = [UIApplication.sharedApplication beginBackgroundTaskWithName:@"jumpman-import" expirationHandler:^{
                // The system's grace period (about 30 s) is over: the app is about to be suspended. The import resumes
                // by itself when the user comes back; if iOS kills the app instead, the next launch starts it again.
                JMLog(@"background task expired; the system will suspend the app");
                [UIApplication.sharedApplication endBackgroundTask:g_bg];
                g_bg = UIBackgroundTaskInvalid;
            }];
            JMLog(@"import started: keep-awake and background task on");
        } else if (!on && g_bg != UIBackgroundTaskInvalid) {
            JMLog(@"import ended: background task released");
            [UIApplication.sharedApplication endBackgroundTask:g_bg];
            g_bg = UIBackgroundTaskInvalid;
        }
    });
}

// ------------------------------------------------------------------------------------------------ picker / Files

static NSMutableArray<NSString *> *g_picked;
static NSLock *g_picked_lock;
static dispatch_queue_t g_copy_queue;

static NSString *JMInbox(void) {
    NSString *d = [JMDataDir() stringByAppendingPathComponent:@"inbox"];
    [NSFileManager.defaultManager createDirectoryAtPath:d withIntermediateDirectories:YES attributes:nil error:nil];
    return d;
}

// Copies a (security-scoped, possibly iCloud) file into the inbox with file coordination and queues its path.
// Also stores bookmark data next to it so a later update can ask the same file again without the picker.
static void JMIngest(NSURL *url) {
    dispatch_async(g_copy_queue, ^{
        BOOL scoped = [url startAccessingSecurityScopedResource];
        NSFileCoordinator *fc = [[NSFileCoordinator alloc] initWithFilePresenter:nil];
        NSError *err = nil;
        [fc coordinateReadingItemAtURL:url options:NSFileCoordinatorReadingWithoutChanges error:&err byAccessor:^(NSURL *u) {
            NSString *dest = [JMInbox() stringByAppendingPathComponent:u.lastPathComponent];
            [NSFileManager.defaultManager removeItemAtPath:dest error:nil];
            NSError *e2 = nil;
            if ([NSFileManager.defaultManager copyItemAtURL:u toURL:[NSURL fileURLWithPath:dest] error:&e2]) {
                NSData *bm = [u bookmarkDataWithOptions:0 includingResourceValuesForKeys:nil relativeToURL:nil error:nil];
                if (bm) [bm writeToFile:[dest stringByAppendingString:@".bookmark"] atomically:YES];
                [g_picked_lock lock]; [g_picked addObject:dest]; [g_picked_lock unlock];
            } else {
                NSLog(@"jumpman: could not copy %@: %@", u, e2);
            }
        }];
        if (scoped) [url stopAccessingSecurityScopedResource];
    });
}

@interface JMPickerDelegate : NSObject <UIDocumentPickerDelegate>
@end
@implementation JMPickerDelegate
- (void)documentPicker:(UIDocumentPickerViewController *)controller didPickDocumentsAtURLs:(NSArray<NSURL *> *)urls {
    for (NSURL *u in urls) JMIngest(u);
}
@end

static JMPickerDelegate *g_picker_delegate;

static UIViewController *JMTopController(void) {
    UIWindow *key = nil;
    for (UIScene *s in UIApplication.sharedApplication.connectedScenes) {
        if (![s isKindOfClass:UIWindowScene.class]) continue;
        for (UIWindow *w in ((UIWindowScene *)s).windows) if (!key || w.isKeyWindow) key = w;
    }
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    // winit creates its window before any scene exists (main.m re-attaches it): fall back to every app window.
    if (!key.rootViewController) for (UIWindow *w in UIApplication.sharedApplication.windows) if (w.rootViewController && (!key.rootViewController || w.isKeyWindow)) key = w;
#pragma clang diagnostic pop
    UIViewController *vc = key.rootViewController;
    while (vc.presentedViewController && !vc.presentedViewController.isBeingDismissed) vc = vc.presentedViewController;
    return vc;
}

// Shows the system document picker (any file; N64 dumps have no registered type). Results arrive via jm_picked_pop.
static void JMPresentPicker(int attempt) {
    UIViewController *top = JMTopController();
    if (!top || top.view.window == nil) {
        // The game window can still be attaching to its scene right after launch: try again shortly.
        if (attempt < 20) {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.25 * NSEC_PER_SEC)), dispatch_get_main_queue(), ^{ JMPresentPicker(attempt + 1); });
        } else {
            NSLog(@"jumpman: no view controller to present the ROM picker from");
        }
        return;
    }
    if ([top isKindOfClass:UIDocumentPickerViewController.class]) return;  // already showing
    UIDocumentPickerViewController *p = [[UIDocumentPickerViewController alloc] initForOpeningContentTypes:@[UTTypeData] asCopy:NO];
    p.allowsMultipleSelection = YES;
    p.shouldShowFileExtensions = YES;
    if (!g_picker_delegate) g_picker_delegate = [JMPickerDelegate new];
    p.delegate = g_picker_delegate;
    [top presentViewController:p animated:YES completion:nil];
    NSLog(@"jumpman: ROM picker presented");
}

void jm_pick_roms(void) {
    dispatch_async(dispatch_get_main_queue(), ^{ JMPresentPicker(0); });
}

// Pops one picked/opened ROM path (already copied into the sandbox). Returns 1 if `buf` was filled.
int jm_picked_pop(char *buf, size_t cap) {
    [g_picked_lock lock];
    NSString *p = g_picked.firstObject;
    if (p) [g_picked removeObjectAtIndex:0];
    [g_picked_lock unlock];
    if (!p) return 0;
    strlcpy(buf, p.fileSystemRepresentation, cap);
    return 1;
}

// Resolves the bookmark saved with an earlier pick (roms.json "bookmark" -> a fresh inbox copy); returns 1 and the new path.
int jm_rom_from_bookmark(const char *bookmark_file, char *buf, size_t cap) {
    NSData *bm = [NSData dataWithContentsOfFile:[NSString stringWithUTF8String:bookmark_file]];
    if (!bm) return 0;
    BOOL stale = NO;
    NSURL *u = [NSURL URLByResolvingBookmarkData:bm options:0 relativeToURL:nil bookmarkDataIsStale:&stale error:nil];
    if (!u) return 0;
    JMIngest(u);
    return 1;
}

// application:openURL:options: (Files "Open in...", AirDrop, share sheet). winit's app delegate does not implement it, so
// the method is added at launch.
static BOOL JMOpenURL(id self, SEL _cmd, UIApplication *app, NSURL *url, NSDictionary *opts) {
    if (url.isFileURL) JMIngest(url);
    return YES;
}

__attribute__((constructor)) static void jm_import_boot(void) {
    g_bg = UIBackgroundTaskInvalid;
    g_picked = [NSMutableArray new];
    g_picked_lock = [NSLock new];
    g_copy_queue = dispatch_queue_create("jumpman.import.copy", DISPATCH_QUEUE_SERIAL);
    [[NSNotificationCenter defaultCenter] addObserverForName:UIApplicationDidFinishLaunchingNotification object:nil queue:nil usingBlock:^(NSNotification *n) {
        // Files only lists the app ("On My iPhone > Jumpman") once its Documents folder has something in it.
        NSString *docs = NSSearchPathForDirectoriesInDomains(NSDocumentDirectory, NSUserDomainMask, YES).firstObject;
        [NSFileManager.defaultManager createDirectoryAtPath:docs withIntermediateDirectories:YES attributes:nil error:nil];
        NSString *note = [docs stringByAppendingPathComponent:@"Put your N64 ROMs here.txt"];
        if (![NSFileManager.defaultManager fileExistsAtPath:note]) {
            [@"Copy your own Super Mario 64, Mario Kart 64, Ocarina of Time (1.2) and Banjo-Kazooie US ROM dumps (.z64 / .v64 / .n64) into this folder, then tap SCAN in Jumpman. Or tap CHOOSE ROM FILES in Jumpman to pick them from anywhere in Files.\n" writeToFile:note atomically:YES encoding:NSUTF8StringEncoding error:nil];
        }
        id delegate = UIApplication.sharedApplication.delegate;
        if (delegate && ![delegate respondsToSelector:@selector(application:openURL:options:)]) {
            class_addMethod([delegate class], @selector(application:openURL:options:), (IMP)JMOpenURL, "c@:@@@");
        }
    }];
}
