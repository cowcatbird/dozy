// Thin C surface over the macOS display APIs Dozy uses, so the Rust side never deals with Objective-C.
#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>
#include <signal.h>
#include <dlfcn.h>
#include <stdbool.h>

// ---- Night Shift (private CoreBrightness.framework) ----

typedef struct { int hour; int minute; } CBTime;
typedef struct { CBTime from; CBTime to; } CBSchedule;
typedef struct {
    BOOL active;
    BOOL enabled;
    BOOL sunSchedulePermitted;
    int mode;
    CBSchedule schedule;
    unsigned long long disableFlags;
    BOOL available;
} CBStatus;
typedef struct { float min; float max; float mid; } CBRange;

@protocol DozyBlueLightClient
- (BOOL)getBlueLightStatus:(CBStatus *)status;
- (BOOL)getStrength:(float *)strength;
- (BOOL)getCCTRange:(CBRange *)range;
- (BOOL)setEnabled:(BOOL)enabled;
- (BOOL)setStrength:(float)strength commit:(BOOL)commit;
- (void)setStatusNotificationBlock:(void (^)(void))block;
@end

static id<DozyBlueLightClient> blue_light(void) {
    static id<DozyBlueLightClient> client;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        dlopen("/System/Library/PrivateFrameworks/CoreBrightness.framework/CoreBrightness", RTLD_NOW);
        Class cls = NSClassFromString(@"CBBlueLightClient");
        if (cls) client = (id<DozyBlueLightClient>)[[cls alloc] init];
    });
    return client;
}

typedef struct {
    bool available;
    bool enabled;
    float strength;
    float min_kelvin;
    float max_kelvin;
} DozyNightShift;

bool dozy_ns_read(DozyNightShift *out) {
    id<DozyBlueLightClient> c = blue_light();
    if (!c) return false;
    CBStatus status = {0};
    CBRange range = {0};
    float strength = 0;
    if (![c getBlueLightStatus:&status] || ![c getStrength:&strength]) return false;
    if (![c getCCTRange:&range] || range.min <= 0) range = (CBRange){2700, 6000, 4100};
    *out = (DozyNightShift){status.available, status.enabled, strength, range.min, range.max};
    return true;
}

bool dozy_ns_set_enabled(bool enabled) { return [blue_light() setEnabled:enabled]; }
bool dozy_ns_set_strength(float strength) { return [blue_light() setStrength:strength commit:YES]; }

// ---- Gamma (public CoreGraphics) ----

uint32_t dozy_displays(uint32_t *ids, uint32_t max) {
    uint32_t count = 0;
    if (CGGetOnlineDisplayList(max, ids, &count) != kCGErrorSuccess) return 0;
    return count;
}

bool dozy_gamma_get(uint32_t display, float gains[3]) {
    CGGammaValue rmin, rmax, rgam, gmin, gmax, ggam, bmin, bmax, bgam;
    if (CGGetDisplayTransferByFormula(display, &rmin, &rmax, &rgam, &gmin, &gmax, &ggam, &bmin, &bmax, &bgam) != kCGErrorSuccess)
        return false;
    gains[0] = rmax;
    gains[1] = gmax;
    gains[2] = bmax;
    return true;
}

bool dozy_gamma_set(uint32_t display, const float gains[3]) {
    return CGSetDisplayTransferByFormula(display, 0, gains[0], 1, 0, gains[1], 1, 0, gains[2], 1) == kCGErrorSuccess;
}

void dozy_gamma_reset(void) { CGDisplayRestoreColorSyncSettings(); }

// ---- Color filter (MediaAccessibility.framework, unexported-in-headers prefs API) ----

enum { kFilterCategoryColor = 1, kUniversalAccessDMagic = 0x8001 };

typedef struct {
    bool enabled;
    int64_t type;
    double hue;
    double intensity;
} DozyFilter;

typedef struct {
    bool (*get_enabled)(long);
    void (*set_enabled)(long, bool);
    long (*get_type)(long);
    void (*set_type)(long, long);
    double (*get_hue)(long);
    void (*set_hue)(long, double);
    double (*get_intensity)(long);
    void (*set_intensity)(long, double);
    void (*uad_start)(int);
} FilterApi;

static const FilterApi *filter_api(void) {
    static FilterApi api;
    static bool ok;
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        void *ma = dlopen("/System/Library/Frameworks/MediaAccessibility.framework/MediaAccessibility", RTLD_NOW);
        void *ua = dlopen("/System/Library/PrivateFrameworks/UniversalAccess.framework/UniversalAccess", RTLD_NOW);
        if (!ma || !ua) return;
        api.get_enabled = dlsym(ma, "MADisplayFilterPrefGetCategoryEnabled");
        api.set_enabled = dlsym(ma, "MADisplayFilterPrefSetCategoryEnabled");
        api.get_type = dlsym(ma, "MADisplayFilterPrefGetType");
        api.set_type = dlsym(ma, "MADisplayFilterPrefSetType");
        api.get_hue = dlsym(ma, "MADisplayFilterPrefGetSingleColorHue");
        api.set_hue = dlsym(ma, "MADisplayFilterPrefSetSingleColorHue");
        api.get_intensity = dlsym(ma, "MADisplayFilterPrefGetSingleColorIntensity");
        api.set_intensity = dlsym(ma, "MADisplayFilterPrefSetSingleColorIntensity");
        api.uad_start = dlsym(ua, "_UniversalAccessDStart");
        ok = api.get_enabled && api.set_enabled && api.get_type && api.set_type && api.get_hue && api.set_hue &&
             api.get_intensity && api.set_intensity && api.uad_start;
    });
    return ok ? &api : NULL;
}

bool dozy_filter_read(DozyFilter *out) {
    const FilterApi *api = filter_api();
    if (!api) return false;
    *out = (DozyFilter){
        api->get_enabled(kFilterCategoryColor),
        api->get_type(kFilterCategoryColor),
        api->get_hue(kFilterCategoryColor),
        api->get_intensity(kFilterCategoryColor),
    };
    return true;
}

bool dozy_filter_write(const DozyFilter *f) {
    const FilterApi *api = filter_api();
    if (!api) return false;
    api->set_type(kFilterCategoryColor, f->type);
    api->set_hue(kFilterCategoryColor, f->hue);
    api->set_intensity(kFilterCategoryColor, f->intensity);
    api->set_enabled(kFilterCategoryColor, f->enabled);
    // universalaccessd owns the display filter; this wakes it so it picks up the new prefs.
    api->uad_start(kUniversalAccessDMagic);
    return true;
}

// ---- Events, so Dozy can sleep instead of polling ----

static void (*on_change)(void);

static void display_reconfigured(CGDirectDisplayID display, CGDisplayChangeSummaryFlags flags, void *info) {
    if (!(flags & kCGDisplayBeginConfigurationFlag)) on_change();
}

static void darwin_notified(CFNotificationCenterRef c, void *o, CFNotificationName n, const void *obj, CFDictionaryRef info) {
    on_change();
}

// Calls `callback` whenever something may have undone Dozy's look: a display was added, removed or
// reconfigured, the Mac or its screens woke, the user session came back, Night Shift's status changed,
// or the Accessibility color filter changed. Callbacks can arrive on any thread.
void dozy_watch(void (*callback)(void)) {
    on_change = callback;
    CGDisplayRegisterReconfigurationCallback(display_reconfigured, NULL);

    NSNotificationCenter *workspace = NSWorkspace.sharedWorkspace.notificationCenter;
    for (NSNotificationName name in @[ NSWorkspaceDidWakeNotification, NSWorkspaceScreensDidWakeNotification,
                                       NSWorkspaceSessionDidBecomeActiveNotification ]) {
        [workspace addObserverForName:name object:nil queue:nil usingBlock:^(NSNotification *n) { on_change(); }];
    }

    [blue_light() setStatusNotificationBlock:^{ on_change(); }];

    CFNotificationCenterAddObserver(CFNotificationCenterGetDarwinNotifyCenter(), NULL, darwin_notified,
                                    CFSTR("com.apple.mediaaccessibility.displayFilterSettingsChanged"), NULL,
                                    CFNotificationSuspensionBehaviorDeliverImmediately);
}

// Routes SIGTERM, SIGINT and SIGHUP (kill, logout, Ctrl-C) to `callback` on the main queue
// instead of killing the process, so Dozy can hand the display back first.
void dozy_on_terminate(void (*callback)(void)) {
    static dispatch_source_t sources[3];
    int signals[3] = {SIGTERM, SIGINT, SIGHUP};
    for (int i = 0; i < 3; i++) {
        signal(signals[i], SIG_IGN);
        sources[i] = dispatch_source_create(DISPATCH_SOURCE_TYPE_SIGNAL, signals[i], 0, dispatch_get_main_queue());
        dispatch_source_set_event_handler(sources[i], ^{ callback(); });
        dispatch_resume(sources[i]);
    }
}

void *dozy_awake_hold(void) {
    id token = [NSProcessInfo.processInfo beginActivityWithOptions:NSActivityIdleDisplaySleepDisabled | NSActivityUserInitiated
                                                            reason:@"Keep screen awake"];
    return (__bridge_retained void *)token;
}

void dozy_awake_release(void *token) {
    [NSProcessInfo.processInfo endActivity:(__bridge_transfer id)token];
}

double dozy_idle_secs(void) {
    return CGEventSourceSecondsSinceLastEventType(kCGEventSourceStateHIDSystemState, kCGAnyInputEventType);
}

void dozy_nudge(void) {
    CGEventRef here = CGEventCreate(NULL);
    CGPoint p = CGEventGetLocation(here);
    CFRelease(here);
    CGEventSourceRef src = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
    CGEventRef move = CGEventCreateMouseEvent(src, kCGEventMouseMoved, p, kCGMouseButtonLeft);
    CGEventPost(kCGHIDEventTap, move);
    CFRelease(move);
    CFRelease(src);
}

// ---- Run loop: NSApplication itself, so Dozy needs no windowing library ----

void dozy_loop_run(void (*on_start)(void)) {
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
    dispatch_async(dispatch_get_main_queue(), ^{ on_start(); });
    [NSApp run];
}

void dozy_loop_post(void (*callback)(void *), void *context) {
    dispatch_async_f(dispatch_get_main_queue(), context, callback);
}

// One timer, re-armed for each sleep. The leeway lets macOS batch the wake with other work.
void dozy_loop_wake_in(double seconds, void (*on_timer)(void)) {
    static dispatch_source_t timer;
    if (!timer) {
        timer = dispatch_source_create(DISPATCH_SOURCE_TYPE_TIMER, 0, 0, dispatch_get_main_queue());
        dispatch_source_set_event_handler(timer, ^{ on_timer(); });
        dispatch_resume(timer);
    }
    dispatch_source_set_timer(timer, dispatch_time(DISPATCH_TIME_NOW, (int64_t)(seconds * NSEC_PER_SEC)),
                              DISPATCH_TIME_FOREVER, NSEC_PER_SEC);
}
