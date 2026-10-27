#import <CoreFoundation/CoreFoundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>
#import <Vision/Vision.h>

static NSDictionary *rect_dictionary(CGRect rect) {
    return @{
        @"left": @(CGRectGetMinX(rect)),
        @"top": @(CGRectGetMinY(rect)),
        @"right": @(CGRectGetMaxX(rect)),
        @"bottom": @(CGRectGetMaxY(rect)),
    };
}

static void emit_json(NSDictionary *payload, int status) {
    NSError *error = nil;
    NSData *data = [NSJSONSerialization dataWithJSONObject:payload options:0 error:&error];
    if (data != nil) {
        fwrite(data.bytes, 1, data.length, stdout);
        fputc('\n', stdout);
    } else {
        fprintf(stderr, "cannot encode Vision OCR JSON: %s\n", error.localizedDescription.UTF8String);
        status = 1;
    }
    fflush(stdout);
    exit(status);
}

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        if (argc != 2) {
            emit_json(@{@"error": @"usage: vision_ocr <image-path>"}, 2);
        }

        NSString *path = [NSString stringWithUTF8String:argv[1]];
        NSURL *url = [NSURL fileURLWithPath:path];
        CGImageSourceRef source = CGImageSourceCreateWithURL((__bridge CFURLRef)url, NULL);
        if (source == NULL) {
            emit_json(@{@"error": [NSString stringWithFormat:@"cannot open image: %@", path]}, 1);
        }
        CGImageRef image = CGImageSourceCreateImageAtIndex(source, 0, NULL);
        CFRelease(source);
        if (image == NULL) {
            emit_json(@{@"error": [NSString stringWithFormat:@"cannot decode image: %@", path]}, 1);
        }

        CFAbsoluteTime started = CFAbsoluteTimeGetCurrent();
        VNRecognizeTextRequest *request = [[VNRecognizeTextRequest alloc] init];
        request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
        request.recognitionLanguages = @[@"zh-Hans", @"en-US"];
        request.usesLanguageCorrection = NO;
        request.minimumTextHeight = 0.006;

        NSError *error = nil;
        VNImageRequestHandler *handler =
            [[VNImageRequestHandler alloc] initWithCGImage:image options:@{}];
        if (![handler performRequests:@[request] error:&error]) {
            NSString *message = error.localizedDescription ?: @"Vision OCR failed";
            emit_json(@{@"error": message}, 1);
        }

        CGFloat image_width = (CGFloat)CGImageGetWidth(image);
        CGFloat image_height = (CGFloat)CGImageGetHeight(image);
        NSMutableArray *blocks = [NSMutableArray array];
        for (VNRecognizedTextObservation *observation in request.results) {
            VNRecognizedText *candidate = [observation topCandidates:1].firstObject;
            if (candidate == nil) {
                continue;
            }
            NSString *text = [candidate.string
                stringByTrimmingCharactersInSet:[NSCharacterSet whitespaceAndNewlineCharacterSet]];
            if (text.length == 0) {
                continue;
            }
            CGRect normalized = observation.boundingBox;
            CGRect pixel = CGRectMake(
                CGRectGetMinX(normalized) * image_width,
                (1.0 - CGRectGetMaxY(normalized)) * image_height,
                CGRectGetWidth(normalized) * image_width,
                CGRectGetHeight(normalized) * image_height
            );
            [blocks addObject:@{
                @"text": text,
                @"confidence": @(candidate.confidence),
                @"bounds": rect_dictionary(pixel),
                @"normalizedBounds": rect_dictionary(normalized),
            }];
        }

        CFRelease(image);
        UInt64 elapsed_ms = (UInt64)((CFAbsoluteTimeGetCurrent() - started) * 1000.0);
        emit_json(@{
            @"engine": @"apple-vision",
            @"width": @(image_width),
            @"height": @(image_height),
            @"elapsedMs": @(elapsed_ms),
            @"blocks": blocks,
        }, 0);
    }
    return 0;
}
