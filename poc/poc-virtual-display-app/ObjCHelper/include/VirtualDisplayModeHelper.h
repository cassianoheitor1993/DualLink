// VirtualDisplayModeHelper.h
// ObjC helper to create CGVirtualDisplayMode instances from Swift.
// CGVirtualDisplayMode's init takes primitive types (UInt32, UInt32, Double)
// which can't be passed through NSObject.perform() in Swift.

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

/// Creates a CGVirtualDisplayMode instance via Objective-C runtime.
/// Returns nil if the class is not available on this macOS version.
NS_RETURNS_RETAINED
id _Nullable DualLinkCreateVirtualDisplayMode(NSUInteger width, NSUInteger height, double refreshRate);

NS_ASSUME_NONNULL_END
