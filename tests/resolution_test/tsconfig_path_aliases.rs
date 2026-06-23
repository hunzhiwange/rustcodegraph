use super::*;

ignored_backend_test!(
    resolves_an_aliased_import_to_the_alias_mapped_file_not_same_named_elsewhere,
    "resolves an aliased import to the alias-mapped file (not a same-named file elsewhere)"
);
ignored_backend_test!(
    falls_back_gracefully_when_tsconfig_is_absent,
    "falls back gracefully when tsconfig is absent"
);
