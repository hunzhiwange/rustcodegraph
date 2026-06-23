mod describe_7014_r_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "R Extraction";
    const TS_DESCRIBE_LINE: usize = 7014;
    #[test]
    fn describes_112_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 7014);
    }
    mod describe_7015_language_detection {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Language detection";
        const TS_DESCRIBE_LINE: usize = 7015;
        #[test]
        fn describes_113_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 7015);
        }
        #[test]
        fn case_7016_should_detect_r_files_both_extension_cases() {
            let suite = ["R Extraction", "Language detection"];
            assert_eq!(suite.len(), 2);
            assert_eq!(365, 365);
            assert_detected_language("analysis.R", None, Language::R);
            assert_detected_language("scripts/clean.r", None, Language::R);
        }
        #[test]
        fn case_7021_should_report_r_as_supported() {
            let suite = ["R Extraction", "Language detection"];
            assert_eq!(suite.len(), 2);
            assert_eq!(366, 366);
            assert_language_support(Language::R, true);
            assert_supported_languages_include(&[Language::R]);
        }
    }
    mod describe_7027_function_extraction {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Function extraction";
        const TS_DESCRIBE_LINE: usize = 7027;
        #[test]
        fn describes_114_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 7027);
        }
        #[test]
        fn case_7028_extracts_every_assignment_form_lambdas_and_nested_functions() {
            let suite = ["R Extraction", "Function extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(367, 367);
            let code = r#"
clean_data <- function(df, threshold = 0.5) {
  helper <- function(d) scale(d)
  helper(df)
}
normalize = function(v) (v - mean(v)) / sd(v)
double_it <- \(x) x * 2
"#;
            let result = extract("analysis.R", code);
            let funcs = names_by_kind(&result, NodeKind::Function);
            assert_contains(&funcs, "clean_data");
            assert_contains(&funcs, "normalize");
            assert_contains(&funcs, "double_it");
            assert_contains(&funcs, "helper");
            let clean_data = find_node(&result, NodeKind::Function, "clean_data")
                .expect("clean_data function should be extracted");
            assert_eq!(clean_data.language, Language::R);
            assert_eq!(
                clean_data.signature.as_deref(),
                Some("(df, threshold = 0.5)")
            );
        }
        #[test]
        fn case_7048_attributes_body_calls_to_the_enclosing_function() {
            let suite = ["R Extraction", "Function extraction"];
            assert_eq!(suite.len(), 2);
            assert_eq!(368, 368);
            let code = r#"
prep <- function(d) scale(d)
fit_model <- function(data) {
  lm(y ~ x, data = prep(data))
}
"#;
            let result = extract("models.R", code);
            let prep_call = result.unresolved_references.iter().find(|reference| {
                reference.reference_name == "prep"
                    && reference.reference_kind == ReferenceKind::Calls
            });
            let fit_model = find_node(&result, NodeKind::Function, "fit_model")
                .expect("fit_model function should be extracted");
            assert_eq!(
                prep_call.map(|reference| reference.from_node_id.as_str()),
                Some(fit_model.id.as_str())
            );
        }
    }
    mod describe_7065_imports {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Imports";
        const TS_DESCRIBE_LINE: usize = 7065;
        #[test]
        fn describes_115_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 7065);
        }
        #[test]
        fn case_7066_extracts_library_require_source_as_imports_not_calls() {
            let suite = ["R Extraction", "Imports"];
            assert_eq!(suite.len(), 2);
            assert_eq!(369, 369);
            let code = r#"
library(dplyr)
require(stats)
requireNamespace("jsonlite")
source("helpers.R")
"#;
            let result = extract("main.R", code);
            let imports = names_by_kind(&result, NodeKind::Import);
            assert_contains(&imports, "dplyr");
            assert_contains(&imports, "stats");
            assert_contains(&imports, "jsonlite");
            assert_contains(&imports, "helpers.R");
            let lib_calls = result.unresolved_references.iter().filter(|reference| {
                reference.reference_kind == ReferenceKind::Calls
                    && matches!(reference.reference_name.as_str(), "library" | "source")
            });
            assert_eq!(lib_calls.count(), 0);
        }
    }
    mod describe_7087_variables_and_constants {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Variables and constants";
        const TS_DESCRIBE_LINE: usize = 7087;
        #[test]
        fn describes_116_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 7087);
        }
        #[test]
        fn case_7088_extracts_top_level_assignments_all_caps_as_constants_right_assign_too() {
            let suite = ["R Extraction", "Variables and constants"];
            assert_eq!(suite.len(), 2);
            assert_eq!(370, 370);
            let code = r#"
ALPHA <- 0.05
max_iter = 100
compute_stats(df) -> stats_result
inner <- function() {
  local_var <- 1
}
"#;
            let result = extract("config.R", code);
            let constant = find_node(&result, NodeKind::Constant, "ALPHA")
                .expect("ALPHA constant should be extracted");
            assert_eq!(constant.kind, NodeKind::Constant);
            let variable = find_node(&result, NodeKind::Variable, "max_iter")
                .expect("max_iter variable should be extracted");
            assert_eq!(variable.kind, NodeKind::Variable);
            let right_assigned = find_node(&result, NodeKind::Variable, "stats_result")
                .expect("stats_result variable should be extracted");
            assert_eq!(right_assigned.kind, NodeKind::Variable);
            assert!(!result.nodes.iter().any(|node| node.name == "local_var"));
        }
    }
    mod describe_7109_classes {
        use super::*;
        const TS_DESCRIBE_TITLE: &str = "Classes";
        const TS_DESCRIBE_LINE: usize = 7109;
        #[test]
        fn describes_117_is_represented() {
            assert!(!TS_DESCRIBE_TITLE.is_empty());
            assert_eq!(TS_DESCRIBE_LINE, 7109);
        }
        #[test]
        fn case_7110_extracts_s4_r5_r6_class_calls_as_classes_with_their_list_methods() {
            let suite = ["R Extraction", "Classes"];
            assert_eq!(suite.len(), 2);
            assert_eq!(371, 371);
            let code = r#"
setClass("Patient", representation(id = "character"))
Account <- setRefClass("Account",
  fields = list(balance = "numeric"),
  methods = list(deposit = function(x) { balance <<- balance + x })
)
Stack <- R6Class("Stack",
  public = list(push = function(v) invisible(v))
)
setGeneric("describe", function(obj) standardGeneric("describe"))
setMethod("describe", "Patient", function(obj) paste(obj@id))
"#;
            let result = extract("classes.R", code);
            let classes = names_by_kind(&result, NodeKind::Class);
            assert_contains(&classes, "Patient");
            assert_contains(&classes, "Account");
            assert_contains(&classes, "Stack");
            let methods = names_by_kind(&result, NodeKind::Method);
            assert_contains(&methods, "deposit");
            assert_contains(&methods, "push");
            let describes = result
                .nodes
                .iter()
                .filter(|node| node.name == "describe" && node.kind == NodeKind::Function)
                .count();
            assert!(describes >= 2);
            assert!(find_node(&result, NodeKind::Variable, "Account").is_none());
        }
        #[test]
        fn case_7138_extracts_ggproto_classes_with_direct_arg_methods_and_the_parent_as_ext() {
            let suite = ["R Extraction", "Classes"];
            assert_eq!(suite.len(), 2);
            assert_eq!(372, 372);
            let code = r#"
GeomPoint <- ggproto("GeomPoint", Geom,
  required_aes = c("x", "y"),
  draw_panel = function(data, panel_params, coord) {
    coords <- coord$transform(data, panel_params)
    grid::pointsGrob(coords$x, coords$y)
  },
  draw_key = draw_key_point
)
"#;
            let result = extract("geom-point.R", code);
            let class = find_node(&result, NodeKind::Class, "GeomPoint")
                .expect("GeomPoint class should be extracted");
            find_node(&result, NodeKind::Method, "draw_panel")
                .expect("draw_panel method should be extracted");
            let extends = result.unresolved_references.iter().find(|reference| {
                reference.reference_kind == ReferenceKind::Extends
                    && reference.reference_name == "Geom"
            });
            assert_eq!(
                extends.map(|reference| reference.from_node_id.as_str()),
                Some(class.id.as_str())
            );
            assert!(find_node(&result, NodeKind::Variable, "GeomPoint").is_none());
        }
    }
}
