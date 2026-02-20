// DualLinkVirtualDisplayHelper.m
#import "include/DualLinkVirtualDisplayHelper.h"
#import <objc/runtime.h>
#import <objc/message.h>

id _Nullable DualLinkCreateDisplayMode(NSUInteger width, NSUInteger height, double refresh) {
    Class modeClass = NSClassFromString(@"CGVirtualDisplayMode");
    if (!modeClass) { return nil; }

    SEL allocSel = sel_registerName("alloc");
    SEL initSel  = sel_registerName("initWithWidth:height:refreshRate:");

    if (![modeClass instancesRespondToSelector:initSel]) { return nil; }

    id obj = ((id(*)(Class, SEL))objc_msgSend)(modeClass, allocSel);
    if (!obj) { return nil; }

    // Signature: (id self, SEL _cmd, NSUInteger width, NSUInteger height, double refreshRate)
    typedef id (*InitFn)(id, SEL, NSUInteger, NSUInteger, double);
    return ((InitFn)objc_msgSend)(obj, initSel, width, height, refresh);
}
