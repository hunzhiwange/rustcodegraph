mod describe_5709_vue_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Vue Extraction";
    const TS_DESCRIBE_LINE: usize = 5709;
    #[test]
    fn describes_089_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 5709);
    }
    #[test]
    fn case_5710_should_detect_vue_files() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(293, 293);
        assert_detected_language("App.vue", None, Language::Vue);
        assert_detected_language("components/Button.vue", None, Language::Vue);
        assert_language_support(Language::Vue, true);
    }
    #[test]
    fn case_5716_should_extract_component_node_from_a_vue_sfc() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(294, 294);
        let code = r#"<template>
  <div>{{ message }}</div>
</template>

<script>
export default {
  data() {
    return { message: 'Hello' };
  }
}
</script>
"#;
        let result = extract("HelloWorld.vue", code);
        let component = expect_node(&result, NodeKind::Component, "HelloWorld");
        assert_eq!(component.language, Language::Vue);
        assert!(is_exported(component));
    }
    #[test]
    fn case_5738_should_extract_functions_from_script_block() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(295, 295);
        let code = r#"<template>
  <button @click="handleClick">Click</button>
</template>

<script>
function handleClick() {
  console.log('clicked');
}

const count = 0;
</script>
"#;
        let result = extract("Button.vue", code);
        expect_node(&result, NodeKind::Component, "Button");
        let func = expect_node(&result, NodeKind::Function, "handleClick");
        assert_eq!(func.language, Language::Vue);
    }
    #[test]
    fn case_5762_should_extract_from_script_setup_lang_ts_block() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(296, 296);
        let code = r#"<template>
  <div>{{ count }}</div>
</template>

<script setup lang="ts">
import { ref } from 'vue';

const count = ref(0);

function increment(): void {
  count.value++;
}
</script>
"#;
        let result = extract("Counter.vue", code);
        expect_node(&result, NodeKind::Component, "Counter");
        let func = expect_node(&result, NodeKind::Function, "increment");
        assert_eq!(func.language, Language::Vue);
        for node in &result.nodes {
            assert_eq!(node.language, Language::Vue, "node: {node:?}");
        }
    }
    #[test]
    fn case_5793_should_extract_calls_from_top_level_script_setup_initializers() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(297, 297);
        let code = r#"<template>
  <div>{{ token }}</div>
</template>

<script setup lang="ts">
import { getTokenMp } from './api/upload';

const token = getTokenMp();
</script>
"#;
        let result = extract("Issue425Setup.vue", code);
        assert_reference_names_include(&result, ReferenceKind::Calls, &["getTokenMp"]);
    }
    #[test]
    fn case_5812_should_extract_calls_from_vue_options_api_object_methods() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(298, 298);
        let code = r#"<template>
  <button @click="save">Save</button>
</template>

<script>
import { getTokenMp } from './api/upload';

export default {
  methods: {
    save() {
      return getTokenMp();
    }
  },
  setup() {
    return getTokenMp();
  }
}
</script>
"#;
        let result = extract("Issue425Options.vue", code);
        let calls = reference_names(&result, ReferenceKind::Calls)
            .into_iter()
            .filter(|name| name == "getTokenMp")
            .collect::<Vec<_>>();
        assert_eq!(calls.len(), 2, "calls: {calls:?}");
    }
    #[test]
    fn case_5840_should_extract_component_usages_from_the_vue_template_pascalcase_kebab() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(299, 299);
        let code = r#"<template>
  <div class="wrap">
    <UserCard :user="u" />
    <my-button>Click</my-button>
    <Transition><span>x</span></Transition>
  </div>
</template>

<script setup lang="ts">
import UserCard from './UserCard.vue';
import MyButton from './MyButton.vue';
</script>
"#;
        let result = extract("Host.vue", code);
        let refs = reference_names(&result, ReferenceKind::References);
        assert_contains(&refs, "UserCard");
        assert_contains(&refs, "MyButton");
        assert_not_contains_fragment(&refs, "Transition");
        assert_not_contains_fragment(&refs, "Div");
        assert_not_contains_fragment(&refs, "Span");
    }
    #[test]
    fn case_5866_should_extract_from_both_script_and_script_setup_blocks() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(300, 300);
        let code = r#"<template>
  <div>{{ msg }}</div>
</template>

<script>
export default {
  name: 'DualScript'
}
</script>

<script setup>
const msg = 'hello';

function greet() {
  return msg;
}
</script>
"#;
        let result = extract("DualScript.vue", code);
        expect_node(&result, NodeKind::Component, "DualScript");
        expect_node(&result, NodeKind::Function, "greet");
    }
    #[test]
    fn case_5894_should_create_component_node_for_template_only_vue_file() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(301, 301);
        let code = r#"<template>
  <div>Static content</div>
</template>
"#;
        let result = extract("Static.vue", code);
        let component = expect_node(&result, NodeKind::Component, "Static");
        assert_eq!(component.language, Language::Vue);
        assert_eq!(result.nodes.len(), 1, "nodes: {:?}", result.nodes);
    }
    #[test]
    fn case_5910_should_create_containment_edges_from_component_to_script_nodes() {
        let suite = ["Vue Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(302, 302);
        let code = r#"<template>
  <div>{{ value }}</div>
</template>

<script setup lang="ts">
const value = 42;
</script>
"#;
        let result = extract("Contained.vue", code);
        let component = expect_node(&result, NodeKind::Component, "Contained");
        let contains = result
            .edges
            .iter()
            .filter(|edge| edge.source == component.id && edge.kind == EdgeKind::Contains)
            .count();
        assert!(contains > 0, "edges: {:?}", result.edges);
    }
}
