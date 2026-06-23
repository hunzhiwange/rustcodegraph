//! Rust port of `__tests__/strip-comments.test.ts`.

use regex::Regex;
use rustcodegraph::resolution::strip_comments::{CommentLang, strip_comments_for_regex};

fn assert_matches(text: &str, pattern: &str) {
    let regex = Regex::new(pattern).expect("test regex should compile");
    assert!(
        regex.is_match(text),
        "expected output to match /{pattern}/:\n{text}"
    );
}

fn assert_not_matches(text: &str, pattern: &str) {
    let regex = Regex::new(pattern).expect("test regex should compile");
    assert!(
        !regex.is_match(text),
        "expected output not to match /{pattern}/:\n{text}"
    );
}

mod strip_comments_for_regex {
    use super::*;

    #[test]
    fn python_strips_line_comments() {
        let src = "x = 1  # path('/fake/', View)\nreal = 2";
        let out = strip_comments_for_regex(src, CommentLang::Python);
        assert_not_matches(&out, r"path\('/fake/");
        assert_matches(&out, r"real = 2");
    }

    #[test]
    fn python_strips_triple_quoted_docstrings() {
        let src = "\"\"\"\npath('/in-docstring/', View)\n\"\"\"\nreal = 1\n";
        let out = strip_comments_for_regex(src, CommentLang::Python);
        assert_not_matches(&out, r"in-docstring");
        assert_matches(&out, r"real = 1");
    }

    #[test]
    fn python_keeps_hash_inside_strings() {
        let src = "path('#/fragment/', View)\n";
        let out = strip_comments_for_regex(src, CommentLang::Python);
        assert!(out.contains("'#/fragment/'"));
    }

    #[test]
    fn python_handles_triple_single_quoted_docstrings() {
        let src = "'''\npath('/fake/')\n'''\nreal = 1\n";
        let out = strip_comments_for_regex(src, CommentLang::Python);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r"real = 1");
    }

    #[test]
    fn typescript_strips_line_and_block_comments() {
        let src = "// app.get('/fake', x)\n/* app.get('/also-fake', y) */\napp.get('/real', z)";
        let out = strip_comments_for_regex(src, CommentLang::TypeScript);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r"'/real'");
    }

    #[test]
    fn typescript_keeps_double_slash_inside_strings() {
        let src = "const url = \"https://example.com/path\";\n";
        let out = strip_comments_for_regex(src, CommentLang::TypeScript);
        assert!(out.contains("https://example.com/path"));
    }

    #[test]
    fn php_strips_line_hash_and_block_comments() {
        let src = "// Route::get('/a', X::class)\n# Route::get('/b', Y::class)\n/* Route::get('/c', Z::class) */\nReal::go();";
        let out = strip_comments_for_regex(src, CommentLang::Php);
        assert_not_matches(&out, r"'/a'");
        assert_not_matches(&out, r"'/b'");
        assert_not_matches(&out, r"'/c'");
        assert!(out.contains("Real::go();"));
    }

    #[test]
    fn ruby_strips_begin_end() {
        let src = "=begin\nget '/fake', to: 'x#y'\n=end\nget '/real', to: 'a#b'\n";
        let out = strip_comments_for_regex(src, CommentLang::Ruby);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r"'/real'");
    }

    #[test]
    fn ruby_strips_hash_comments() {
        let src = "# get '/fake', to: 'x#y'\nget '/real', to: 'a#b'\n";
        let out = strip_comments_for_regex(src, CommentLang::Ruby);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r"'/real'");
    }

    #[test]
    fn rust_handles_nested_block_comments() {
        let src = r#"/* outer /* inner */ still in outer */ .route("/real", get(h))"#;
        let out = strip_comments_for_regex(src, CommentLang::Rust);
        assert_not_matches(&out, r"inner");
        assert_matches(&out, r"/real");
    }

    #[test]
    fn go_keeps_backtick_raw_strings_intact_strips_line_comments() {
        let src = "// r.GET(\"/fake\", h)\nr.GET(`/real`, h2)\n";
        let out = strip_comments_for_regex(src, CommentLang::Go);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r"`/real`");
    }

    #[test]
    fn go_strips_block_comments_containing_route_shaped_text() {
        let src = "/* r.GET(\"/fake\", h) */\nr.GET(\"/real\", h2)\n";
        let out = strip_comments_for_regex(src, CommentLang::Go);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r#""/real""#);
    }

    #[test]
    fn java_strips_line_and_block_comments() {
        let src = "// @GetMapping(\"/fake\")\n/* @PostMapping(\"/also-fake\") */\n@GetMapping(\"/real\")\n";
        let out = strip_comments_for_regex(src, CommentLang::Java);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r#""/real""#);
    }

    #[test]
    fn csharp_strips_line_and_block_comments() {
        let src =
            "// [HttpGet(\"/fake\")]\n/* [HttpPost(\"/also-fake\")] */\n[HttpGet(\"/real\")]\n";
        let out = strip_comments_for_regex(src, CommentLang::CSharp);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r#""/real""#);
    }

    #[test]
    fn swift_strips_line_and_block_comments() {
        let src = "// app.get(\"fake\", use: x)\n/* app.get(\"also-fake\", use: y) */\napp.get(\"real\", use: z)\n";
        let out = strip_comments_for_regex(src, CommentLang::Swift);
        assert_not_matches(&out, r"fake");
        assert_matches(&out, r#""real""#);
    }

    #[test]
    fn preserves_line_numbers_newlines_retained() {
        let src = "line1\n# comment with path('/fake/')\nline3";
        let out = strip_comments_for_regex(src, CommentLang::Python);
        let lines = out.split('\n').collect::<Vec<_>>();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2], "line3");
    }

    #[test]
    fn preserves_overall_length_so_source_offsets_stay_valid() {
        let src = "x = 1  # path('/fake/', View)\nreal = 2";
        let out = strip_comments_for_regex(src, CommentLang::Python);
        assert_eq!(out.len(), src.len());
    }
}
