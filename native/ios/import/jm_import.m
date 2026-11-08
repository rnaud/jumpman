// iOS ROM import host (BYOR build only): the embedded CPython that runs byor/pipeline.py in-process, the document
// picker / Files integration, and the keep-awake + background-task handling around a long extraction.
// Bridged to Rust (src/byor_ios.rs) with plain C functions, like the controller bridge (src/controller/gc_shim.m).
#define PY_SSIZE_T_CLEAN
#import <UIKit/UIKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#include <Python.h>
#include <objc/runtime.h>
#include <sys/resource.h>

extern PyMODINIT_FUNC PyInit__jtools(void);

// ------------------------------------------------------------------------------------------------ Python

static int g_py_ready = 0;
static NSLock *g_py_lock;

// Start the interpreter once. `home` holds lib/python3.13 (+ lib-dynload with .fwork stubs), `byor_dir` is
// <bundle>/byor-src/byor (pipeline.py, inproc.py). Returns 0 on success.
int jm_py_start(const char *home, const char *byor_dir) {
    static dispatch_once_t once;
    __block int rc = 0;
    dispatch_once(&once, ^{
        // iOS starts processes with a soft limit of 256 descriptors; the in-process exporters hold many files at once.
        struct rlimit rl;
        if (getrlimit(RLIMIT_NOFILE, &rl) == 0) { rl.rlim_cur = MIN(10240, rl.rlim_max); setrlimit(RLIMIT_NOFILE, &rl); }
        g_py_lock = [NSLock new];
        PyImport_AppendInittab("_jtools", PyInit__jtools);
        // exporters call read_text() without an encoding: force UTF-8 like the Mac locale gives them
        PyPreConfig pre;
        PyPreConfig_InitIsolatedConfig(&pre);
        pre.utf8_mode = 1;
        Py_PreInitialize(&pre);
        PyConfig config;
        PyConfig_InitIsolatedConfig(&config);
        config.write_bytecode = 0;      // the bundle is read-only; scripts are compiled in memory
        config.install_signal_handlers = 0;
        config.buffered_stdio = 0;
        PyStatus st = PyConfig_SetBytesString(&config, &config.home, home);
        if (!PyStatus_Exception(st)) st = Py_InitializeFromConfig(&config);
        PyConfig_Clear(&config);
        if (PyStatus_Exception(st)) { NSLog(@"jumpman: Python failed to start: %s", st.err_msg ? st.err_msg : "?"); rc = 1; return; }
        PyObject *path = PySys_GetObject("path");
        PyObject *p = PyUnicode_FromString(byor_dir);
        PyList_Insert(path, 0, p);
        Py_DECREF(p);
        PyEval_SaveThread();            // release the GIL: any thread re-enters with PyGILState_Ensure
        g_py_ready = 1;
    });
    return rc ? rc : (g_py_ready ? 0 : 2);
}

// Run `pipeline.main()` with sys.argv = ["pipeline.py"] + args (JSON array of strings); its stdout/stderr go to `out_fd`.
// Blocks until finished (call from a worker thread); calls are serialised. Returns the exit status.
int jm_py_call(const char *json_args, int out_fd) {
    if (!g_py_ready) return 99;
    [g_py_lock lock];
    PyGILState_STATE gil = PyGILState_Ensure();
    PyObject *main_mod = PyImport_AddModule("__main__");
    PyObject *globals = PyModule_GetDict(main_mod);
    PyObject *a = PyUnicode_FromString(json_args);
    PyDict_SetItemString(globals, "__jm_args", a); Py_DECREF(a);
    PyObject *fd = PyLong_FromLong(out_fd);
    PyDict_SetItemString(globals, "__jm_fd", fd); Py_DECREF(fd);
    PyRun_SimpleString(
        "import sys, os, json, traceback\n"
        "__jm_rc = 1\n"
        "__jm_out = os.fdopen(__jm_fd, 'w', buffering=1, closefd=False)\n"
        "sys.stdout = sys.stderr = __jm_out\n"
        "sys.argv = ['pipeline.py'] + json.loads(__jm_args)\n"
        "try:\n"
        "    import pipeline, inproc\n"
        "    inproc.CANCEL.clear()\n"
        "    __jm_rc = pipeline.main()\n"
        "except SystemExit as e:\n"
        "    __jm_rc = e.code if isinstance(e.code, int) else (0 if e.code is None else 1)\n"
        "except BaseException as e:\n"
        "    if type(e).__name__ == 'Cancelled':\n"
        "        print('@@FAIL all cancelled', flush=True)\n"
        "    else:\n"
        "        traceback.print_exc(file=__jm_out)\n"
        "        print('@@FAIL all internal error: ' + repr(e)[-300:], flush=True)\n"
        "    __jm_rc = 1\n"
        "__jm_out.flush()\n"
        "sys.stdout = sys.__stdout__; sys.stderr = sys.__stderr__\n");
    PyObject *rc = PyDict_GetItemString(globals, "__jm_rc");
    int result = rc ? (int)PyLong_AsLong(rc) : 1;
    PyGILState_Release(gil);
    [g_py_lock unlock];
    return result;
}

// Ask the running extraction to stop at its next output line (the previous install is left untouched).
void jm_py_cancel(void) {
    if (!g_py_ready) return;
    PyGILState_STATE gil = PyGILState_Ensure();
    PyRun_SimpleString("import inproc; inproc.CANCEL.set()");
    PyGILState_Release(gil);
}

// ------------------------------------------------------------------------------------------------ paths

static NSString *JMDataDir(void) {
    NSString *env = NSProcessInfo.processInfo.environment[@"JUMPMAN_DATA_DIR"];
    if (env.length) return env;
    NSURL *support = [NSFileManager.defaultManager URLsForDirectory:NSApplicationSupportDirectory inDomains:NSUserDomainMask].firstObject;
    return [support.path stringByAppendingPathComponent:@"Jumpman"];
}

// Creates <Application Support>/Jumpman and keeps it out of iCloud / device backups (the extracted data is ~300 MB
// and can be regenerated from the ROMs).
const char *jm_prepare_data_dir(void) {
    static NSString *dir; static dispatch_once_t once;
    dispatch_once(&once, ^{
        dir = JMDataDir();
        [NSFileManager.defaultManager createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:nil];
        [[NSURL fileURLWithPath:dir] setResourceValue:@YES forKey:NSURLIsExcludedFromBackupKey error:nil];
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
// gracefully (on expiry the extraction is cancelled; its atomic stage means the previous install stays intact).
void jm_keep_awake(int on) {
    dispatch_async(dispatch_get_main_queue(), ^{
        UIApplication.sharedApplication.idleTimerDisabled = on ? YES : NO;
        if (on && g_bg == UIBackgroundTaskInvalid) {
            g_bg = [UIApplication.sharedApplication beginBackgroundTaskWithName:@"jumpman-import" expirationHandler:^{
                jm_py_cancel();
                [UIApplication.sharedApplication endBackgroundTask:g_bg];
                g_bg = UIBackgroundTaskInvalid;
            }];
        } else if (!on && g_bg != UIBackgroundTaskInvalid) {
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
    UIViewController *vc = key.rootViewController;
    while (vc.presentedViewController) vc = vc.presentedViewController;
    return vc;
}

// Shows the system document picker (any file; N64 dumps have no registered type). Results arrive via jm_picked_pop.
void jm_pick_roms(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        UIViewController *top = JMTopController();
        if (!top) return;
        UIDocumentPickerViewController *p = [[UIDocumentPickerViewController alloc] initForOpeningContentTypes:@[UTTypeData] asCopy:NO];
        p.allowsMultipleSelection = YES;
        if (!g_picker_delegate) g_picker_delegate = [JMPickerDelegate new];
        p.delegate = g_picker_delegate;
        [top presentViewController:p animated:YES completion:nil];
    });
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
        id delegate = UIApplication.sharedApplication.delegate;
        if (delegate && ![delegate respondsToSelector:@selector(application:openURL:options:)]) {
            class_addMethod([delegate class], @selector(application:openURL:options:), (IMP)JMOpenURL, "c@:@@@");
        }
    }];
}
