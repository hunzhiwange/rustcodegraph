//! Conservative Java argument and functional-interface constraints.
use crate::resolution::types::{ResolutionContext, ResolvedRef, UnresolvedRef};
use crate::types::{Language, NodeKind, ReferenceKind};

// Keep one parsed source per resolver thread; overload checks in the same
// file share the native tree without retaining every project file's AST.
thread_local! {
    static LAST_TREE: std::cell::RefCell<Option<(String, tree_sitter::Tree)>> = const { std::cell::RefCell::new(None) };
}
fn parse(source: &str) -> Option<tree_sitter::Tree> {
    LAST_TREE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some((old_source, tree)) = cache.as_ref() {
            if old_source == source {
                return Some(tree.clone());
            }
        }
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .ok()?;
        let tree = parser.parse(source, None)?;
        *cache = Some((source.to_owned(), tree.clone()));
        Some(tree)
    })
}

fn text<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> &'a str {
    &source[node.byte_range()]
}
fn descendants(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut out = vec![node];
    let mut i = 0;
    while i < out.len() {
        let mut cursor = out[i].walk();
        let children = out[i].named_children(&mut cursor).collect::<Vec<_>>();
        out.extend(children);
        i += 1;
    }
    out
}
fn simple(raw: &str) -> &str {
    raw.trim().rsplit('.').next().unwrap_or(raw.trim())
}
fn declared(name: &str, call: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    let mut scope = call.parent();
    while let Some(node) = scope {
        let mut cursor = node.walk();
        let mut declarations = node.named_children(&mut cursor).collect::<Vec<_>>();
        if let Some(parameters) = node.child_by_field_name("parameters") {
            let mut c = parameters.walk();
            declarations.extend(parameters.named_children(&mut c));
        }
        for n in declarations.into_iter().rev() {
            if !matches!(
                n.kind(),
                "formal_parameter" | "local_variable_declaration" | "field_declaration"
            ) || (n.kind() != "field_declaration" && n.start_byte() >= call.start_byte())
            {
                continue;
            }
            let matches = n
                .child_by_field_name("name")
                .is_some_and(|id| text(id, source) == name)
                || descendants(n).into_iter().any(|d| {
                    d.kind() == "variable_declarator"
                        && d.child_by_field_name("name")
                            .is_some_and(|id| text(id, source) == name)
                });
            if matches {
                return n
                    .child_by_field_name("type")
                    .map(|t| text(t, source).to_owned());
            }
        }
        if node.kind() == "class_declaration" {
            break;
        }
        scope = node.parent();
    }
    None
}

fn parameters(signature: &str) -> Vec<&str> {
    let Some((_, rest)) = signature.split_once('(') else {
        return vec![];
    };
    let Some((rest, _)) = rest.rsplit_once(')') else {
        return vec![];
    };
    let mut depth = 0usize;
    let mut start = 0;
    let mut out = vec![];
    for (i, c) in rest.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(rest[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    if !rest[start..].trim().is_empty() {
        out.push(rest[start..].trim());
    }
    out
}

fn conversion(parameter: &str, argument: &str) -> Option<usize> {
    let p = simple(parameter);
    let a = simple(argument);
    if p == a {
        return Some(0);
    }
    let widen: &[&str] = match a {
        "byte" => &["short", "int", "long", "float", "double"],
        "short" | "char" => &["int", "long", "float", "double"],
        "int" => &["long", "float", "double"],
        "long" => &["float", "double"],
        "float" => &["double"],
        _ => &[],
    };
    if let Some(i) = widen.iter().position(|t| *t == p) {
        return Some(i + 1);
    }
    let boxed = [
        ("boolean", "Boolean"),
        ("byte", "Byte"),
        ("short", "Short"),
        ("char", "Character"),
        ("int", "Integer"),
        ("long", "Long"),
        ("float", "Float"),
        ("double", "Double"),
    ];
    boxed
        .iter()
        .any(|(primitive, wrapper)| {
            (*primitive == a && *wrapper == p) || (*primitive == p && *wrapper == a)
        })
        .then_some(10)
}

fn argument_type(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    Some(match node.kind() {
        "string_literal" => "String".into(),
        "character_literal" => "char".into(),
        "true" | "false" => "boolean".into(),
        "decimal_integer_literal" | "hex_integer_literal" => {
            if text(node, source).ends_with(['l', 'L']) {
                "long"
            } else {
                "int"
            }
            .into()
        }
        "decimal_floating_point_literal" => if text(node, source).ends_with(['f', 'F']) {
            "float"
        } else {
            "double"
        }
        .into(),
        "object_creation_expression" | "cast_expression" => {
            text(node.child_by_field_name("type")?, source).into()
        }
        "identifier" => declared(text(node, source), node, source)?,
        _ => return None,
    })
}

pub(super) fn refine(
    reference: &UnresolvedRef,
    context: &mut dyn ResolutionContext,
    previous: Option<ResolvedRef>,
) -> Option<ResolvedRef> {
    if reference.reference_kind != ReferenceKind::Calls {
        return previous;
    }
    let method_name = reference
        .reference_name
        .rsplit(['.', ':'])
        .next()
        .unwrap_or("");
    if !reference.reference_name.contains('.')
        && context
            .get_nodes_by_name(method_name)
            .iter()
            .filter(|n| n.language == Language::Java && n.kind == NodeKind::Method)
            .count()
            < 2
    {
        return previous;
    }
    let Some(source) = context.read_file(&reference.file_path) else {
        return previous;
    };
    let tree = parse(&source)?;
    let name = reference.reference_name.rsplit(['.', ':']).next()?;
    let point = tree_sitter::Point::new(
        reference.line.saturating_sub(1) as usize,
        reference.column as usize,
    );
    let mut at = tree
        .root_node()
        .named_descendant_for_point_range(point, point);
    let mut call = None;
    while let Some(node) = at {
        if node.kind() == "method_invocation"
            && node.start_position() == point
            && node
                .child_by_field_name("name")
                .is_some_and(|n| text(n, &source) == name)
        {
            call = Some(node);
            break;
        }
        at = node.parent();
    }
    let Some(call) = call else {
        return previous;
    };
    let mut selected = previous;
    // Constrain the declaring class before comparing overload signatures.
    // A prior fuzzy/exact match can otherwise belong to a different class.
    let receiver_type = call.child_by_field_name("object").and_then(|object| {
        if object.kind() != "identifier" {
            return None;
        }
        let receiver = text(object, &source);
        declared(receiver, call, &source).or_else(|| {
            context
                .get_nodes_by_name(receiver)
                .iter()
                .any(|n| n.language == Language::Java && n.kind == NodeKind::Class)
                .then(|| receiver.to_owned())
        })
    });
    let receiver_type = receiver_type.or_else(|| {
        if call.child_by_field_name("object").is_some() {
            return None;
        }
        let mut node = call.parent();
        while let Some(n) = node {
            if n.kind() == "class_declaration" {
                return n
                    .child_by_field_name("name")
                    .map(|n| text(n, &source).to_owned());
            }
            node = n.parent();
        }
        None
    });
    if let Some(ty) = receiver_type {
        let classes = context
            .get_nodes_by_name(simple(&ty))
            .into_iter()
            .filter(|n| n.language == Language::Java && n.kind == NodeKind::Class)
            .collect::<Vec<_>>();
        let local = classes
            .iter()
            .filter(|n| n.file_path == reference.file_path)
            .collect::<Vec<_>>();
        let class = if local.len() == 1 {
            Some(local[0])
        } else if classes.len() == 1 {
            classes.first()
        } else {
            None
        };
        if let Some(class) = class {
            let qualified = format!("{}::{name}", class.qualified_name);
            selected = context
                .get_nodes_by_name(name)
                .into_iter()
                .find(|n| {
                    n.language == Language::Java
                        && n.file_path == class.file_path
                        && n.qualified_name == qualified
                })
                .map(|n| {
                    super::common::resolved(
                        reference,
                        &n.id,
                        0.95,
                        crate::resolution::types::ResolvedBy::InstanceMethod,
                    )
                });
            if selected.is_none() {
                selected = super::typed::resolve_method_on_type(
                    &class.name,
                    name,
                    reference,
                    context,
                    0.9,
                    crate::resolution::types::ResolvedBy::InstanceMethod,
                    None,
                    0,
                );
            }
        } else {
            return None;
        }
    }
    // A lambda receiver is constrained only by a receiver-less, same-file call
    // with one unambiguous JDK Consumer<T> declaration.
    if let Some(receiver) = call.child_by_field_name("object") {
        let mut ancestor = call.parent();
        while let Some(node) = ancestor {
            if node.kind() == "lambda_expression" {
                let parameter = node.child_by_field_name("parameters");
                if parameter.is_some_and(|p| {
                    text(p, &source).trim_matches(['(', ')', ' ']) == text(receiver, &source)
                }) {
                    selected = None;
                    let outer = node.parent().and_then(|a| a.parent());
                    if let Some(outer) = outer.filter(|n| {
                        n.kind() == "method_invocation" && n.child_by_field_name("object").is_none()
                    }) {
                        let outer_name = outer
                            .child_by_field_name("name")
                            .map(|n| text(n, &source))
                            .unwrap_or("");
                        let methods = context
                            .get_nodes_by_name(outer_name)
                            .into_iter()
                            .filter(|n| {
                                n.language == Language::Java && n.file_path == reference.file_path
                            })
                            .collect::<Vec<_>>();
                        if methods.len() == 1 {
                            let sig = methods[0].signature.as_deref().unwrap_or("");
                            let args = outer.child_by_field_name("arguments");
                            let index = args.and_then(|a| {
                                let mut c = a.walk();
                                a.named_children(&mut c).position(|n| n.id() == node.id())
                            });
                            let params = parameters(sig);
                            let param = index.and_then(|i| params.get(i).copied());
                            let functional = param.and_then(|p| p.trim().strip_prefix("Consumer<"));
                            if let Some(rest) = functional
                                .filter(|_| source.contains("import java.util.function.Consumer;"))
                            {
                                if let Some((ty, _)) = rest.split_once('>') {
                                    let targets = context
                                        .get_nodes_by_name(name)
                                        .into_iter()
                                        .filter(|n| {
                                            n.language == Language::Java
                                                && n.qualified_name
                                                    .ends_with(&format!("::{}::{name}", simple(ty)))
                                        })
                                        .collect::<Vec<_>>();
                                    if targets.len() == 1 {
                                        selected = Some(super::common::resolved(
                                            reference,
                                            &targets[0].id,
                                            0.95,
                                            crate::resolution::types::ResolvedBy::InstanceMethod,
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                break;
            }
            ancestor = node.parent();
        }
    }
    let chosen = selected?;
    let Some(target) = context
        .get_nodes_by_name(name)
        .into_iter()
        .find(|n| n.id == chosen.target_node_id)
    else {
        return Some(chosen);
    };
    let siblings = context
        .get_nodes_by_name(name)
        .into_iter()
        .filter(|n| {
            n.language == Language::Java
                && n.kind == NodeKind::Method
                && n.file_path == target.file_path
                && n.qualified_name == target.qualified_name
        })
        .collect::<Vec<_>>();
    if siblings.len() < 2 {
        return Some(chosen);
    }
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return Some(chosen);
    };
    let mut cursor = arguments.walk();
    let types = arguments
        .named_children(&mut cursor)
        .map(|a| argument_type(a, &source))
        .collect::<Option<Vec<_>>>();
    let Some(types) = types else {
        return None;
    };
    let mut fitting = siblings
        .iter()
        .filter_map(|n| {
            if n.visibility == Some(crate::types::Visibility::Private)
                && n.file_path != reference.file_path
            {
                return None;
            }
            let signature = n.signature.as_deref()?;
            let params = parameters(signature);
            if params.len() != types.len() {
                return None;
            }
            let score = params
                .iter()
                .zip(&types)
                .map(|(p, t)| conversion(p.split_whitespace().next()?, t))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .sum::<usize>();
            Some((score, n))
        })
        .collect::<Vec<_>>();
    fitting.sort_by_key(|(score, _)| *score);
    if fitting.is_empty() || (fitting.len() > 1 && fitting[0].0 == fitting[1].0) {
        return None;
    }
    Some(super::common::resolved(
        reference,
        &fitting[0].1.id,
        chosen.confidence,
        chosen.resolved_by,
    ))
}
