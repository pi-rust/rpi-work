#import <CoreImage/CoreImage.h>
#include <stdint.h>

size_t work_qr_modules(const uint8_t *bytes, size_t length, uint8_t *modules, size_t capacity) {
    @autoreleasepool {
        CIFilter *filter = [CIFilter filterWithName:@"CIQRCodeGenerator"];
        [filter setValue:[NSData dataWithBytes:bytes length:length] forKey:@"inputMessage"];
        [filter setValue:@"M" forKey:@"inputCorrectionLevel"];
        CIImage *code = filter.outputImage;
        if (code == nil || CGRectIsEmpty(code.extent) || CGRectIsInfinite(code.extent)) {
            return 0;
        }
        size_t side = (size_t)CGRectGetWidth(code.extent);
        if (side == 0 || side > capacity / side || side != (size_t)CGRectGetHeight(code.extent)) {
            return 0;
        }
        CIContext *renderer = [CIContext contextWithOptions:nil];
        CGImageRef image = [renderer createCGImage:code fromRect:code.extent];
        if (image == NULL) {
            return 0;
        }
        CGColorSpaceRef gray = CGColorSpaceCreateDeviceGray();
        CGContextRef bitmap = CGBitmapContextCreate(modules, side, side, 8, side, gray, (CGBitmapInfo)kCGImageAlphaNone);
        CGColorSpaceRelease(gray);
        if (bitmap != NULL) {
            CGContextSetInterpolationQuality(bitmap, kCGInterpolationNone);
            CGContextDrawImage(bitmap, CGRectMake(0, 0, side, side), image);
            CGContextRelease(bitmap);
        }
        CGImageRelease(image);
        return bitmap == NULL ? 0 : side;
    }
}
