mod describe_6423_objective_c_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Objective-C Extraction";
    const TS_DESCRIBE_LINE: usize = 6423;
    fn objc_sample() -> &'static str {
        r#"
#import <Foundation/Foundation.h>
#import "MyClass.h"

@interface MyClass : NSObject <NSCopying>
@property (nonatomic, copy) NSString *name;
- (void)greet;
- (void)doThing:(id)x with:(id)y;
+ (instancetype)shared;
@end

@implementation MyClass

- (void)greet {
    NSLog(@"Hello");
    [self doWork];
}

- (void)doThing:(id)x with:(id)y {
    [self notify:x];
}

+ (instancetype)shared {
    return [[MyClass alloc] init];
}

@end

void helperFunction(int count) {
    MyClass *obj = [MyClass shared];
    [obj greet];
}
"#
    }

    #[test]
    fn describes_103_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 6423);
    }
    #[test]
    fn case_6458_should_extract_classes_methods_functions_and_imports() {
        let suite = ["Objective-C Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(334, 334);
        let result = extract("App.m", objc_sample());
        assert_names_include(&result, NodeKind::Class, &["MyClass"]);

        let mut methods = names_by_kind(&result, NodeKind::Method);
        methods.sort();
        assert_eq!(
            methods,
            vec![
                "doThing:with:".to_owned(),
                "greet".to_owned(),
                "shared".to_owned()
            ]
        );
        let shared = expect_node(&result, NodeKind::Method, "shared");
        assert_eq!(shared.is_static, Some(true));

        assert_names_include(&result, NodeKind::Property, &["name"]);
        assert_names_include(&result, NodeKind::Function, &["helperFunction"]);
        assert_import_names(&result, &["Foundation/Foundation.h", "MyClass.h"]);
    }
    #[test]
    fn case_6481_should_record_inheritance_and_protocol_conformance() {
        let suite = ["Objective-C Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(335, 335);
        let result = extract("App.m", objc_sample());
        assert_reference_names_include(&result, ReferenceKind::Extends, &["NSObject"]);
        assert_reference_names_include(&result, ReferenceKind::Implements, &["NSCopying"]);
    }
    #[test]
    fn case_6489_should_record_message_sends_and_c_calls() {
        let suite = ["Objective-C Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(336, 336);
        let result = extract("App.m", objc_sample());
        let calls = reference_names(&result, ReferenceKind::Calls);
        for expected in ["NSLog", "doWork", "MyClass.shared", "obj.greet"] {
            assert_contains(&calls, expected);
        }
    }
    #[test]
    fn case_6497_should_reconstruct_multi_keyword_selectors_at_the_call_site_so_they_re() {
        let suite = ["Objective-C Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(337, 337);
        let code = r#"
@implementation Caller
- (void)demo {
    NSMutableDictionary *d = [NSMutableDictionary new];
    [d setObject:@"v" forKey:@"k"];
    [d setObject:@"v2" forKey:@"k2" withRetry:@YES];
    [self touchesBegan:nil withEvent:nil];
}
@end
"#;
        let result = extract("Caller.m", code);
        let calls = reference_names(&result, ReferenceKind::Calls);
        for expected in [
            "d.setObject:forKey:",
            "d.setObject:forKey:withRetry:",
            "touchesBegan:withEvent:",
        ] {
            assert_contains(&calls, expected);
        }
    }
    #[test]
    fn case_6526_should_not_classify_pure_c_headers_with_end_in_comments_as_objc() {
        let suite = ["Objective-C Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(338, 338);
        assert_detected_language(
            "stdio.h",
            Some("/* @end of file */\n#ifndef STDIO_H\nvoid printf(const char *);\n#endif\n"),
            Language::C,
        );
    }
    #[test]
    fn case_6531_should_extract_protocol_declarations() {
        let suite = ["Objective-C Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(339, 339);
        let code = "\n@protocol DataSource <NSObject>\n- (NSInteger)numberOfItems;\n@end\n";
        let result = extract("DataSource.h", code);
        expect_node(&result, NodeKind::Protocol, "DataSource");
    }
    #[test]
    fn case_6542_should_report_objective_c_as_supported() {
        let suite = ["Objective-C Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(340, 340);
        assert_language_support(Language::ObjC, true);
        assert_supported_languages_include(&[Language::ObjC]);
    }
}
