// DualLinkVirtualDisplayHelper.h
// Objective-C bridge for CGVirtualDisplay API.
//
// CGVirtualDisplayMode's initWithWidth:height:refreshRate: takes primitive types
// (NSUInteger, NSUInteger, double) that cannot be passed through Swift's NSObject
// perform() API. This helper wraps that call using objc_msgSend directly.

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

/// Creates a CGVirtualDisplayMode instance with the specified dimensions.
/// Uses objc_msgSend to call initWithWidth:height:refreshRate: with primitive types.
///
/// @param width    Width in pixels (e.g. 1920)
/// @param height   Height in pixels (e.g. 1080)
/// @param refresh  Refresh rate in Hz (e.g. 30.0, 60.0)
/// @return A configured CGVirtualDisplayMode, or nil if unavailable.
NS_RETURNS_RETAINED
id _Nullable DualLinkCreateDisplayMode(NSUInteger width, NSUInteger height, double refresh);

NS_ASSUME_NONNULL_END
