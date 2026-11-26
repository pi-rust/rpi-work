#import <CoreImage/CoreImage.h>
#import <ImageIO/ImageIO.h>
#import <Vision/Vision.h>
#include <stdint.h>
#include <string.h>

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

size_t work_scan_qr(const uint8_t *bytes, size_t length, uint8_t *text, size_t capacity) {
    @autoreleasepool {
        NSData *data = [NSData dataWithBytesNoCopy:(void *)bytes length:length freeWhenDone:NO];
        CGImageSourceRef source = CGImageSourceCreateWithData((__bridge CFDataRef)data, NULL);
        if (source == NULL) {
            return 0;
        }
        NSDictionary *properties = CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, 0, NULL));
        NSUInteger width = [properties[(__bridge NSString *)kCGImagePropertyPixelWidth] unsignedIntegerValue];
        NSUInteger height = [properties[(__bridge NSString *)kCGImagePropertyPixelHeight] unsignedIntegerValue];
        CFRelease(source);
        if (width == 0 || height == 0 || width > 24000000 / height) {
            return 0;
        }
        VNDetectBarcodesRequest *request = [[VNDetectBarcodesRequest alloc] init];
        request.symbologies = @[VNBarcodeSymbologyQR];
        VNImageRequestHandler *reader = [[VNImageRequestHandler alloc] initWithData:data options:@{}];
        if (![reader performRequests:@[request] error:nil] || request.results.count != 1) {
            return 0;
        }
        NSData *payload = [request.results.firstObject.payloadStringValue dataUsingEncoding:NSUTF8StringEncoding];
        if (payload.length == 0 || payload.length > capacity) {
            return 0;
        }
        memcpy(text, payload.bytes, payload.length);
        return payload.length;
    }
}
