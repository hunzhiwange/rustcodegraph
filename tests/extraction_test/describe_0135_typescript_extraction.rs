mod describe_0135_typescript_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "TypeScript Extraction";
    const TS_DESCRIBE_LINE: usize = 135;
    #[test]
    fn describes_003_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 135);
    }
    #[test]
    fn case_0136_should_extract_function_declarations() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(19, 19);
        let code = r#"
export function processPayment(amount: number): Promise<Receipt> {
  return stripe.charge(amount);
}
"#;
        let result = extract("payment.ts", code);
        let file_node = find_node(&result, NodeKind::File, "payment.ts");
        assert!(file_node.is_some());

        let func_node = find_node(&result, NodeKind::Function, "processPayment")
            .expect("processPayment function should be extracted");
        assert_eq!(func_node.language, Language::TypeScript);
        assert!(is_exported(func_node));
        assert!(
            func_node
                .signature
                .as_deref()
                .is_some_and(|signature| signature.contains("amount: number")),
            "signature: {:?}",
            func_node.signature
        );
    }
    #[test]
    fn case_0159_should_extract_class_declarations() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(20, 20);
        let code = r#"
export class PaymentService {
  private stripe: StripeClient;

  constructor(apiKey: string) {
    this.stripe = new StripeClient(apiKey);
  }

  async charge(amount: number): Promise<Receipt> {
    return this.stripe.charge(amount);
  }
}
"#;
        let result = extract("service.ts", code);
        let class_node = find_node(&result, NodeKind::Class, "PaymentService")
            .expect("PaymentService class should be extracted");
        assert!(is_exported(class_node));

        let method_nodes = nodes_by_kind(&result, NodeKind::Method);
        assert!(!method_nodes.is_empty(), "nodes: {:?}", result.nodes);
        assert!(
            method_nodes.iter().any(|method| method.name == "charge"),
            "methods: {:?}",
            method_nodes
                .iter()
                .map(|method| method.name.as_str())
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn case_0187_captures_docstrings_for_export_and_const_wrapped_declarations_780() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(21, 21);
        let code = r#"
// plain class control
class Ledger {}

// exported class
export class Invoice {}

// export default
export default function settle() { return true; }

// exported arrow const
export const refund = (amount: number) => amount;

// non-export arrow const
const audit = (amount: number) => amount;
"#;
        let result = extract("doc.ts", code);
        assert_eq!(
            find_node(&result, NodeKind::Class, "Ledger")
                .and_then(|node| node.docstring.as_deref()),
            Some("plain class control")
        );
        assert_eq!(
            find_node(&result, NodeKind::Class, "Invoice")
                .and_then(|node| node.docstring.as_deref()),
            Some("exported class")
        );
        assert_eq!(
            find_node(&result, NodeKind::Function, "settle")
                .and_then(|node| node.docstring.as_deref()),
            Some("export default")
        );
        assert_eq!(
            find_node(&result, NodeKind::Function, "refund")
                .and_then(|node| node.docstring.as_deref()),
            Some("exported arrow const")
        );
        assert_eq!(
            find_node(&result, NodeKind::Function, "audit")
                .and_then(|node| node.docstring.as_deref()),
            Some("non-export arrow const")
        );
    }
    #[test]
    fn case_0212_does_not_mis_attribute_a_class_comment_to_an_uncommented_member_780() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(22, 22);
        let code = r#"
// Comment for Box
export class Box {
  noComment() {}
  // own comment
  withComment() {}
}
"#;
        let result = extract("box.ts", code);
        assert_eq!(
            find_node(&result, NodeKind::Class, "Box").and_then(|node| node.docstring.as_deref()),
            Some("Comment for Box")
        );
        assert_eq!(
            find_node(&result, NodeKind::Method, "noComment")
                .and_then(|node| node.docstring.as_deref()),
            None
        );
        assert_eq!(
            find_node(&result, NodeKind::Method, "withComment")
                .and_then(|node| node.docstring.as_deref()),
            Some("own comment")
        );
    }
    #[test]
    fn case_0227_captures_docstrings_for_decorated_python_declarations_stripping_780() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(23, 23);
        let code = [
            "# decorated function",
            "@app.route(\"/x\")",
            "def py_handler():",
            "    return 1",
            "",
            "",
            "# plain function control",
            "def py_plain():",
            "    return 1",
            "",
            "",
            "# decorated class",
            "@dataclass",
            "class PyModel:",
            "    pass",
            "",
        ]
        .join("\n");
        let result = extract("mod.py", &code);
        assert_eq!(
            find_node(&result, NodeKind::Function, "py_handler")
                .and_then(|node| node.docstring.as_deref()),
            Some("decorated function")
        );
        assert_eq!(
            find_node(&result, NodeKind::Function, "py_plain")
                .and_then(|node| node.docstring.as_deref()),
            Some("plain function control")
        );
        assert_eq!(
            find_node(&result, NodeKind::Class, "PyModel")
                .and_then(|node| node.docstring.as_deref()),
            Some("decorated class")
        );
    }
    #[test]
    fn case_0252_cleans_comment_markers_across_language_styles_780() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(24, 24);
        let doc = |file: &str, code: &str, kind: NodeKind, name: &str| {
            extract(file, code)
                .nodes
                .into_iter()
                .find(|node| node.kind == kind && node.name == name)
                .and_then(|node| node.docstring)
        };

        assert_eq!(
            doc(
                "m.rs",
                "/// rust doc line\nfn rs_fn() {}",
                NodeKind::Function,
                "rs_fn"
            )
            .as_deref(),
            Some("rust doc line")
        );
        assert_eq!(
            doc(
                "m.lua",
                "-- lua line\nfunction lua_fn() end",
                NodeKind::Function,
                "lua_fn"
            )
            .as_deref(),
            Some("lua line")
        );
        assert_eq!(
            doc(
                "b.lua",
                "--[[ lua block ]]\nfunction lua_b() end",
                NodeKind::Function,
                "lua_b"
            )
            .as_deref(),
            Some("lua block")
        );
        assert_eq!(
            doc(
                "m.java",
                "/* java block */\nclass J {}",
                NodeKind::Class,
                "J"
            )
            .as_deref(),
            Some("java block")
        );
    }
    #[test]
    fn case_0270_should_extract_interfaces() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(25, 25);
        let code = r#"
export interface User {
  id: string;
  name: string;
  email: string;
}
"#;
        let result = extract("types.ts", code);
        assert!(find_node(&result, NodeKind::File, "types.ts").is_some());
        let iface_node = find_node(&result, NodeKind::Interface, "User")
            .expect("User interface should be extracted");
        assert!(is_exported(iface_node));
    }
    #[test]
    fn case_0291_should_extract_type_references_from_interface_property_signatures() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(26, 26);
        let code = r#"
import type { IPage } from '../PromoterList';
import type { IOrderField } from '../types';

interface Hprops {
  value?: Partial<IPage> & Partial<IOrderField>;
}
"#;
        let result = extract("HeaderFilter.ts", code);
        let refs = references_by_kind(&result, ReferenceKind::References);
        assert_contains(&refs, "IPage");
        assert_contains(&refs, "IOrderField");
    }
    #[test]
    fn case_0307_should_extract_type_references_from_interface_method_signatures() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(27, 27);
        let code = r#"
import type { IPage } from '../PromoterList';
import type { IOrderField } from '../types';

interface MethodForm {
  fetchPage(arg: IPage): IOrderField;
}
"#;
        let result = extract("MethodForm.ts", code);
        let refs = references_by_kind(&result, ReferenceKind::References);
        assert_contains(&refs, "IPage");
        assert_contains(&refs, "IOrderField");
    }
    #[test]
    fn case_0323_extracts_type_references_from_in_body_local_variable_annotations() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(28, 28);
        let code = r#"
import { Foo } from './types';

export function build(): void {
  const items: Foo[] = [];
  void items;
}

export class K {
  run(): void {
    const a: Foo = { x: 1 };
    void a;
  }
}

export const handler = {
  handle(): void {
    const b: Foo = { x: 1 };
    void b;
  },
};
"#;
        let result = extract("inbody.ts", code);
        let foo_refs = result
            .unresolved_references
            .iter()
            .filter(|reference| {
                reference.reference_kind == ReferenceKind::References
                    && reference.reference_name == "Foo"
            })
            .collect::<Vec<_>>();
        assert!(
            foo_refs.len() >= 3,
            "expected one Foo ref per body scope, got {foo_refs:?}"
        );
        for reference in foo_refs {
            let owner = result
                .nodes
                .iter()
                .find(|node| node.id == reference.from_node_id)
                .expect("type reference should be attributed to a graph node");
            assert!(
                matches!(owner.kind, NodeKind::Function | NodeKind::Method),
                "unexpected owner for Foo ref: {owner:?}"
            );
        }
        for local in ["items", "a", "b"] {
            assert!(
                result.nodes.iter().all(|node| node.name != local),
                "local variable {local:?} should not be extracted as a node"
            );
        }
    }
    #[test]
    fn case_0371_should_track_function_calls() {
        let suite = ["TypeScript Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(29, 29);
        let code = r#"
function main() {
  const result = processData();
  console.log(result);
}
"#;
        let result = extract("main.ts", code);
        let calls = references_by_kind(&result, ReferenceKind::Calls);
        assert_contains(&calls, "processData");
    }
}
