//! Regression coverage for the generated-file detector that drives
//! symbol-disambiguation down-ranking.
//!
//! This is the Rust port of `__tests__/generated-detection.test.ts`.

use rustcodegraph::extraction::generated_detection::is_generated_file;

mod is_generated_file_tests {
    use super::*;

    #[test]
    fn classifies_go_protobuf_grpc_pulsar_mock_outputs_as_generated() {
        assert!(is_generated_file("api/cosmos/bank/v1beta1/tx_grpc.pb.go"));
        assert!(is_generated_file("x/bank/types/tx.pb.go"));
        assert!(is_generated_file("api/cosmos/bank/v1beta1/tx.pulsar.go"));
        // cosmos-sdk uses `<base>_mocks.go`; mockgen's default is `mock_<src>.go`;
        // many projects use `<base>_mock.go`. All three are mockgen output.
        assert!(is_generated_file(
            "x/auth/testutil/expected_keepers_mocks.go"
        ));
        assert!(is_generated_file("internal/foo_mock.go"));
        assert!(is_generated_file("mock_keeper.go"));
    }

    #[test]
    fn does_not_flag_the_hand_written_keeper_as_generated() {
        assert!(!is_generated_file("x/bank/keeper/msg_server.go"));
        assert!(!is_generated_file("x/bank/keeper/send.go"));
    }

    #[test]
    fn catches_common_cross_language_codegen_suffixes() {
        assert!(is_generated_file("app/foo.generated.ts"));
        assert!(is_generated_file("app/foo.generated.tsx"));
        assert!(is_generated_file("proto/bar_pb2.py"));
        assert!(is_generated_file("proto/bar_pb2_grpc.py"));
        assert!(is_generated_file("lib/baz.pb.cc"));
        assert!(is_generated_file("lib/baz.pb.h"));
        assert!(is_generated_file("lib/quux.g.dart"));
        assert!(is_generated_file("lib/quux.freezed.dart"));
    }

    #[test]
    fn leaves_ordinary_source_files_alone() {
        assert!(!is_generated_file("src/index.ts"));
        assert!(!is_generated_file("src/components/Foo.tsx"));
        assert!(!is_generated_file("lib/main.dart"));
        assert!(!is_generated_file("cmd/server/main.go"));
        assert!(!is_generated_file("app/db.py"));
    }
}
