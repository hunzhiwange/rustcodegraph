mod describe_5932_astro_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Astro Extraction";
    const TS_DESCRIBE_LINE: usize = 5932;
    #[test]
    fn describes_090_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 5932);
    }
    #[test]
    fn case_5933_should_detect_astro_files() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(303, 303);
        assert_detected_language("src/pages/index.astro", None, Language::Astro);
        assert_detected_language("Layout.astro", None, Language::Astro);
        assert_language_support(Language::Astro, true);
    }
    #[test]
    fn case_5939_should_extract_component_node_from_an_astro_file() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(304, 304);
        let code = r#"---
const title = 'Hello';
---
<h1>{title}</h1>
"#;
        let result = extract("Card.astro", code);
        let component = expect_node(&result, NodeKind::Component, "Card");
        assert_eq!(component.language, Language::Astro);
        assert!(is_exported(component));
    }
    #[test]
    fn case_5954_should_extract_frontmatter_symbols_with_correct_line_numbers_768() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(305, 305);
        let code = r#"---
import { formatDate } from '../utils/format';

function getIconNode(name: string): string {
  return name;
}

const { title } = Astro.props;
---
<span>{title}</span>
"#;
        let result = extract("navs.astro", code);
        let function = expect_node(&result, NodeKind::Function, "getIconNode");
        assert_eq!(function.language, Language::Astro);
        assert_eq!(function.start_line, 4);

        let import = nodes_by_kind(&result, NodeKind::Import)
            .into_iter()
            .next()
            .expect("frontmatter import should be extracted");
        assert_eq!(import.start_line, 2);
    }
    #[test]
    fn case_5979_should_extract_exported_getstaticpaths_from_frontmatter() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(306, 306);
        let code = r#"---
export async function getStaticPaths() {
  return [];
}
const { slug } = Astro.params;
---
<p>{slug}</p>
"#;
        let result = extract("[slug].astro", code);
        let function = expect_node(&result, NodeKind::Function, "getStaticPaths");
        assert!(is_exported(function));
    }
    #[test]
    fn case_5995_should_extract_calls_from_template_expressions() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(307, 307);
        let code = r#"---
import { formatDate } from '../utils/format';
const date = new Date();
---
<time>{formatDate(date)}</time>
"#;
        let result = extract("Stamp.astro", code);
        assert!(
            result.unresolved_references.iter().any(|reference| {
                reference.reference_kind == ReferenceKind::Calls
                    && reference.reference_name == "formatDate"
                    && reference.line == 5
            }),
            "references: {:?}",
            result.unresolved_references
        );
    }
    #[test]
    fn case_6010_should_extract_calls_from_a_multiline_expression_opening_line() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(308, 308);
        let code = r#"---
const posts = [];
---
<ul>
  {posts.map((post) => (
    <li>{render(post)}</li>
  ))}
</ul>
"#;
        let result = extract("List.astro", code);
        assert_reference_names_include(&result, ReferenceKind::Calls, &["posts.map", "render"]);
    }
    #[test]
    fn case_6032_should_extract_pascalcase_component_usages_from_the_template() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(309, 309);
        let code = r#"---
import Layout from '../layouts/Layout.astro';
import PostCard from '../components/PostCard.astro';
---
<Layout title="Home">
  <PostCard />
  <Fragment slot="head" />
  <div class="plain-html" />
</Layout>
"#;
        let result = extract("index.astro", code);
        let refs = reference_names(&result, ReferenceKind::References);
        assert_contains(&refs, "Layout");
        assert_contains(&refs, "PostCard");
        assert_not_contains_fragment(&refs, "Fragment");
        assert_not_contains_fragment(&refs, "div");
    }
    #[test]
    fn case_6054_should_not_extract_template_patterns_from_frontmatter_script_or_style_() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(310, 310);
        let code = r#"---
// <FakeComponent /> inside frontmatter comment
const x = { y: maybeCall(1) };
---
<div>real</div>
<script>
  const z = { w: scriptCall(2) };
</script>
<style>
  .a { color: red; }
</style>
"#;
        let result = extract("Guard.astro", code);
        let refs = reference_names(&result, ReferenceKind::References);
        assert_not_contains_fragment(&refs, "FakeComponent");

        let maybe_calls = result
            .unresolved_references
            .iter()
            .filter(|reference| {
                reference.reference_kind == ReferenceKind::Calls
                    && reference.reference_name == "maybeCall"
            })
            .count();
        assert!(
            maybe_calls <= 1,
            "references: {:?}",
            result.unresolved_references
        );
    }
    #[test]
    fn case_6082_should_extract_script_block_symbols_with_correct_line_numbers() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(311, 311);
        let code = r#"---
const a = 1;
---
<div>hi</div>
<script>
function trackView(page: string) {
  console.log(page);
}
</script>
"#;
        let result = extract("Tracker.astro", code);
        let function = expect_node(&result, NodeKind::Function, "trackView");
        assert_eq!(function.start_line, 6);
        assert_eq!(function.language, Language::Astro);
    }
    #[test]
    fn case_6101_should_create_component_node_for_a_frontmatter_less_template_only_file() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(312, 312);
        let code = "<div>Static content</div>\n";
        let result = extract("Static.astro", code);
        let component = expect_node(&result, NodeKind::Component, "Static");
        assert_eq!(component.language, Language::Astro);
    }
    #[test]
    fn case_6112_should_treat_an_unclosed_frontmatter_fence_as_no_frontmatter() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(313, 313);
        let code = r#"---
const broken = true;
<div>never closed</div>
"#;
        let result = extract("Broken.astro", code);
        expect_node(&result, NodeKind::Component, "Broken");
        assert!(
            result.nodes.iter().all(|node| node.name != "broken"),
            "nodes: {:?}",
            result.nodes
        );
    }
    #[test]
    fn case_6126_should_create_containment_edges_from_component_to_frontmatter_nodes() {
        let suite = ["Astro Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(314, 314);
        let code = r#"---
const value = 42;
---
<div>{value}</div>
"#;
        let result = extract("Contained.astro", code);
        let component = expect_node(&result, NodeKind::Component, "Contained");
        let contains = result
            .edges
            .iter()
            .filter(|edge| edge.source == component.id && edge.kind == EdgeKind::Contains)
            .count();
        assert!(contains > 0, "edges: {:?}", result.edges);
    }
}
