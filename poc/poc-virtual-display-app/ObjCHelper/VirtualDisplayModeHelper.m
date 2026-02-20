// VirtualDisplayModeHelper.m
#import "include/VirtualDisplayModeHelper.h"
#import <objc/runtime.h>
#import <objc/message.h>

id _Nullable DualLinkCreateVirtualDisplayMode(NSUInteger width, NSUInteger height, double refreshRate) {
    Class modeClass = NSClassFromString(@"CGVirtualDisplayMode");
    if (!modeClass) { return nil; }

    SEL allocSel = sel_registerName("alloc");
    SEL initSel  = sel_registerName("initWithWidth:height:refreshRate:");

    if (![modeClass instancesRespondToSelector:initSel]) { return nil; }

    // alloc
    id obj = ((id(*)(Class, SEL))objc_msgSend)(modeClass, allocSel);
    if (!obj) { return nil; }

    // initWithWidth:height:refreshRate: — last arg is double, use objc_msgSend directly
    typedef id (*InitFn)(id, SEL, NSUInteger, NSUInteger, double);
    InitFn initFn = (InitFn)objc_msgSend;
    return initFn(obj, initSel, width, height, refreshRate);
}
