//! Context Builder tests.
//!
//! This is the Rust port of `__tests__/context.test.ts`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::Value;

use rustcodegraph::types::{
    BuildContextOptions, ContextFormat, FindRelevantContextOptions, NodeKind, TaskInput,
    TaskInputDetails,
};
use rustcodegraph::{BuildContextResult, CodeGraph, IndexOptions};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

fn create_temp_dir(prefix: &str) -> PathBuf {
    for attempt in 0..100 {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "{prefix}{}-{unique}-{counter}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return path,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => panic!("failed to create temp dir {}: {err}", path.display()),
        }
    }

    panic!("failed to create unique temp dir with prefix {prefix}");
}

struct TempDir {
    root: PathBuf,
}

impl TempDir {
    fn new(prefix: &str) -> Self {
        Self {
            root: create_temp_dir(prefix),
        }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative_path: &str, contents: &str) {
        let path = self.root.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("failed to create {}: {err}", parent.display()));
        }
        fs::write(&path, contents)
            .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

struct ContextBuilderFixture {
    _temp: TempDir,
    cg: CodeGraph,
}

impl ContextBuilderFixture {
    fn new() -> Self {
        let temp = TempDir::new("codegraph-context-test-");

        temp.write(
            "src/payment.ts",
            r#"/**
 * Payment Service
 * Handles payment processing logic.
 */

export interface PaymentResult {
  success: boolean;
  transactionId: string;
  amount: number;
}

export class PaymentService {
  private apiKey: string;

  constructor(apiKey: string) {
    this.apiKey = apiKey;
  }

  /**
   * Process a payment for the given amount
   */
  async processPayment(amount: number): Promise<PaymentResult> {
    // Validate amount
    if (amount <= 0) {
      throw new Error('Invalid amount');
    }

    // Process payment
    const transactionId = this.generateTransactionId();
    return {
      success: true,
      transactionId,
      amount,
    };
  }

  private generateTransactionId(): string {
    return 'txn_' + Math.random().toString(36).substring(2);
  }
}

export function createPaymentService(apiKey: string): PaymentService {
  return new PaymentService(apiKey);
}
"#,
        );

        temp.write(
            "src/checkout.ts",
            r#"/**
 * Checkout Controller
 * Handles the checkout flow.
 */

import { PaymentService, PaymentResult } from './payment';

export interface CartItem {
  id: string;
  name: string;
  price: number;
  quantity: number;
}

export class CheckoutController {
  private paymentService: PaymentService;

  constructor(paymentService: PaymentService) {
    this.paymentService = paymentService;
  }

  /**
   * Process checkout for the given cart
   */
  async processCheckout(cart: CartItem[]): Promise<PaymentResult> {
    const total = this.calculateTotal(cart);

    if (total === 0) {
      throw new Error('Cart is empty');
    }

    return this.paymentService.processPayment(total);
  }

  /**
   * Calculate the total price of the cart
   */
  calculateTotal(cart: CartItem[]): number {
    return cart.reduce((sum, item) => sum + item.price * item.quantity, 0);
  }
}
"#,
        );

        temp.write(
            "src/utils.ts",
            r#"/**
 * Utility functions
 */

export function formatCurrency(amount: number): string {
  return '$' + amount.toFixed(2);
}

export function validateEmail(email: string): boolean {
  return email.includes('@');
}
"#,
        );

        let mut cg = CodeGraph::init_sync(temp.path()).expect("CodeGraph should initialize");
        let result = cg.index_all(IndexOptions::default());
        assert!(result.success, "indexing failed: {:?}", result.errors);

        Self { _temp: temp, cg }
    }
}

impl Drop for ContextBuilderFixture {
    fn drop(&mut self) {
        self.cg.destroy();
    }
}

fn build_options(format: ContextFormat) -> BuildContextOptions {
    BuildContextOptions {
        max_nodes: None,
        max_code_blocks: None,
        max_code_block_size: None,
        include_code: None,
        format: Some(format),
        search_limit: None,
        traversal_depth: None,
        min_score: None,
    }
}

fn find_options() -> FindRelevantContextOptions {
    FindRelevantContextOptions {
        search_limit: None,
        traversal_depth: None,
        max_nodes: None,
        min_score: None,
        edge_kinds: None,
        node_kinds: None,
    }
}

fn formatted_text(result: BuildContextResult) -> String {
    match result {
        BuildContextResult::Formatted(text) => text,
        BuildContextResult::Context(_) => panic!("context should be formatted text"),
    }
}

mod get_code {
    use super::*;

    #[test]
    fn should_extract_code_for_a_node() {
        let mut fixture = ContextBuilderFixture::new();

        let nodes = fixture.cg.get_nodes_by_kind(NodeKind::Class);
        let payment_service = nodes
            .iter()
            .find(|node| node.name == "PaymentService")
            .expect("PaymentService should be indexed");

        let code = fixture.cg.get_code(&payment_service.id);

        assert!(code.is_some(), "code should not be null");
        let code = code.expect("PaymentService code should be present");
        assert!(code.contains("class PaymentService"));
        assert!(code.contains("processPayment"));
    }

    #[test]
    fn should_return_null_for_non_existent_node() {
        let mut fixture = ContextBuilderFixture::new();

        let code = fixture.cg.get_code("non-existent-id");

        assert!(code.is_none());
    }
}

mod find_relevant_context {
    use super::*;

    #[test]
    fn should_find_relevant_nodes_for_a_query() {
        let mut fixture = ContextBuilderFixture::new();

        let result = fixture.cg.find_relevant_context("PaymentService", None);

        assert!(!result.nodes.is_empty());
        let node_names = result
            .nodes
            .values()
            .map(|node| node.name.clone())
            .collect::<Vec<_>>();
        assert!(
            node_names.iter().any(|name| {
                let lower = name.to_ascii_lowercase();
                lower.contains("payment") || lower.contains("checkout")
            }),
            "expected payment or checkout related nodes, got {node_names:?}"
        );
    }

    #[test]
    fn should_include_edges_in_the_result() {
        let mut fixture = ContextBuilderFixture::new();
        let mut options = find_options();
        options.traversal_depth = Some(2);

        let result = fixture.cg.find_relevant_context("checkout", Some(options));

        let _edges = result.edges;
    }

    #[test]
    fn should_respect_max_nodes_option() {
        let mut fixture = ContextBuilderFixture::new();
        let mut options = find_options();
        options.max_nodes = Some(5);

        let result = fixture.cg.find_relevant_context("function", Some(options));

        assert!(result.nodes.len() <= 5);
    }
}

mod build_context {
    use super::*;

    #[test]
    fn should_build_context_with_markdown_format() {
        let mut fixture = ContextBuilderFixture::new();
        let mut options = build_options(ContextFormat::Markdown);
        options.max_code_blocks = Some(3);

        let result = fixture.cg.build_context(
            TaskInput::Query("Fix checkout error".to_owned()),
            Some(options),
        );
        let markdown = formatted_text(result);

        assert!(markdown.contains("## Code Context"));
        assert!(markdown.contains("**Query:** Fix checkout error"));
    }

    #[test]
    fn should_build_context_with_json_format() {
        let mut fixture = ContextBuilderFixture::new();

        let result = fixture.cg.build_context(
            TaskInput::Query("payment processing".to_owned()),
            Some(build_options(ContextFormat::Json)),
        );
        let parsed: Value =
            serde_json::from_str(&formatted_text(result)).expect("context JSON should parse");

        assert_eq!(parsed["query"], "payment processing");
        assert!(parsed.get("nodes").is_some());
        assert!(parsed["nodes"].is_array());
    }

    #[test]
    fn should_accept_object_input_with_title_and_description() {
        let mut fixture = ContextBuilderFixture::new();

        let result = fixture.cg.build_context(
            TaskInput::Details(TaskInputDetails {
                title: "Checkout bug".to_owned(),
                description: Some("Cart total calculation is wrong".to_owned()),
            }),
            Some(build_options(ContextFormat::Markdown)),
        );
        let markdown = formatted_text(result);

        assert!(markdown.contains("Checkout bug: Cart total calculation is wrong"));
    }

    #[test]
    fn should_include_code_blocks_when_requested() {
        let mut fixture = ContextBuilderFixture::new();
        let mut options = build_options(ContextFormat::Markdown);
        options.include_code = Some(true);
        options.max_code_blocks = Some(2);

        let result = fixture
            .cg
            .build_context(TaskInput::Query("PaymentService".to_owned()), Some(options));
        let markdown = formatted_text(result);

        assert!(markdown.contains("### Code"));
        assert!(markdown.contains("```typescript"));
    }

    #[test]
    fn should_exclude_code_blocks_when_requested() {
        let mut fixture = ContextBuilderFixture::new();
        let mut options = build_options(ContextFormat::Markdown);
        options.include_code = Some(false);

        let result = fixture
            .cg
            .build_context(TaskInput::Query("payment".to_owned()), Some(options));
        let markdown = formatted_text(result);

        assert!(!markdown.contains("### Code"));
    }

    #[test]
    fn should_include_related_symbols_in_compact_format() {
        let mut fixture = ContextBuilderFixture::new();
        let mut options = build_options(ContextFormat::Markdown);
        options.max_nodes = Some(10);

        let result = fixture
            .cg
            .build_context(TaskInput::Query("checkout".to_owned()), Some(options));
        let markdown = formatted_text(result);

        assert!(markdown.contains("### Entry Points"));
    }

    #[test]
    fn should_have_compact_output_without_verbose_stats_footer() {
        let mut fixture = ContextBuilderFixture::new();

        let result = fixture.cg.build_context(
            TaskInput::Query("payment".to_owned()),
            Some(build_options(ContextFormat::Markdown)),
        );
        let markdown = formatted_text(result);
        let verbose_footer =
            Regex::new(r"\*Context:.*symbols.*relationships.*files").expect("valid regex");

        assert!(!verbose_footer.is_match(&markdown));
        assert!(markdown.contains("**Query:**"));
    }
}

mod context_structure {
    use super::*;

    #[test]
    fn should_find_entry_points_from_search() {
        let mut fixture = ContextBuilderFixture::new();

        let result = fixture.cg.build_context(
            TaskInput::Query("PaymentService".to_owned()),
            Some(build_options(ContextFormat::Json)),
        );
        let parsed: Value =
            serde_json::from_str(&formatted_text(result)).expect("context JSON should parse");

        assert!(parsed.get("entryPoints").is_some());
        assert!(
            !parsed["entryPoints"]
                .as_array()
                .expect("entryPoints should be an array")
                .is_empty()
        );
    }

    #[test]
    fn should_traverse_graph_from_entry_points() {
        let mut fixture = ContextBuilderFixture::new();
        let mut options = build_options(ContextFormat::Json);
        options.traversal_depth = Some(2);

        let result = fixture.cg.build_context(
            TaskInput::Query("CheckoutController".to_owned()),
            Some(options),
        );
        let parsed: Value =
            serde_json::from_str(&formatted_text(result)).expect("context JSON should parse");
        let node_names = parsed["nodes"]
            .as_array()
            .expect("nodes should be an array")
            .iter()
            .filter_map(|node| node["name"].as_str())
            .collect::<Vec<_>>();

        assert!(
            node_names.iter().any(|name| name.contains("Checkout")),
            "expected a Checkout node, got {node_names:?}"
        );
    }
}

mod edge_cases {
    use super::*;

    #[test]
    fn should_handle_empty_query() {
        let mut fixture = ContextBuilderFixture::new();

        let result = fixture.cg.build_context(
            TaskInput::Query(String::new()),
            Some(build_options(ContextFormat::Markdown)),
        );

        let _markdown = formatted_text(result);
    }

    #[test]
    fn should_handle_query_with_no_matches() {
        let mut fixture = ContextBuilderFixture::new();

        let result = fixture.cg.build_context(
            TaskInput::Query("xyznonexistent123".to_owned()),
            Some(build_options(ContextFormat::Json)),
        );
        let parsed: Value =
            serde_json::from_str(&formatted_text(result)).expect("context JSON should parse");

        assert!(parsed.get("nodes").is_some());
    }

    #[test]
    fn should_truncate_long_code_blocks() {
        let mut fixture = ContextBuilderFixture::new();
        let mut options = build_options(ContextFormat::Markdown);
        options.max_code_block_size = Some(100);
        options.include_code = Some(true);

        let result = fixture
            .cg
            .build_context(TaskInput::Query("PaymentService".to_owned()), Some(options));
        let markdown = formatted_text(result);

        if markdown.contains("```typescript") {
            assert!(!markdown.is_empty());
        }
    }
}
