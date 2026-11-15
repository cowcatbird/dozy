// Dev tool: prints what the display is doing right now, so Dozy's effect can be checked without eyeballing.
// Build: clang -fobjc-arc -framework Foundation -framework CoreGraphics tools/inspect.m -o target/inspect
// Usage: target/inspect            print state
//        target/inspect ns-off     turn Night Shift off (to watch Dozy turn it back on)
//        target/inspect ns-on      turn Night Shift on (to watch Gamma mode turn it back off)
//        target/inspect gamma-reset  reset every display's gamma, like macOS does after some events
#import <Foundation/Foundation.h>
#import <CoreGraphics/CoreGraphics.h>
#include <dlfcn.h>
#include <string.h>

typedef struct { int hour; int minute; } CBTime;
typedef struct { BOOL active; BOOL enabled; BOOL sunSchedulePermitted; int mode; CBTime from, to; unsigned long long flags; BOOL available; } CBStatus;
@protocol BL
- (BOOL)getBlueLightStatus:(CBStatus *)s;
- (BOOL)getStrength:(float *)s;
- (BOOL)setEnabled:(BOOL)e;
@end

int main(int argc, char **argv) {
    dlopen("/System/Library/PrivateFrameworks/CoreBrightness.framework/CoreBrightness", RTLD_NOW);
    id<BL> c = (id<BL>)[[NSClassFromString(@"CBBlueLightClient") alloc] init];
    if (argc > 1 && !strcmp(argv[1], "ns-off")) { [c setEnabled:NO]; }
    if (argc > 1 && !strcmp(argv[1], "ns-on")) { [c setEnabled:YES]; }
    if (argc > 1 && !strcmp(argv[1], "gamma-reset")) { CGDisplayRestoreColorSyncSettings(); }
    CBStatus s = {0}; float strength = 0;
    [c getBlueLightStatus:&s]; [c getStrength:&strength];
    printf("night_shift enabled=%d strength=%.3f\n", s.enabled, strength);

    CGGammaValue v[9];
    CGGetDisplayTransferByFormula(CGMainDisplayID(), &v[0], &v[1], &v[2], &v[3], &v[4], &v[5], &v[6], &v[7], &v[8]);
    printf("gamma max r=%.3f g=%.3f b=%.3f\n", v[1], v[4], v[7]);

    void *ma = dlopen("/System/Library/Frameworks/MediaAccessibility.framework/MediaAccessibility", RTLD_NOW);
    bool (*en)(long) = dlsym(ma, "MADisplayFilterPrefGetCategoryEnabled");
    long (*ty)(long) = dlsym(ma, "MADisplayFilterPrefGetType");
    double (*hue)(long) = dlsym(ma, "MADisplayFilterPrefGetSingleColorHue");
    double (*in)(long) = dlsym(ma, "MADisplayFilterPrefGetSingleColorIntensity");
    printf("filter enabled=%d type=%ld hue=%.3f intensity=%.3f\n", en(1), ty(1), hue(1), in(1));
}
