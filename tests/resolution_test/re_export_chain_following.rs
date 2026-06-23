use super::*;

ignored_backend_test!(
    chases_a_3_hop_barrel_chain_wildcard_named_declaration,
    "chases a 3-hop barrel chain (wildcard -> named -> declaration)"
);
ignored_backend_test!(
    follows_a_renamed_named_re_export,
    "follows a renamed named re-export (export { foo as bar } from ...)"
);
ignored_backend_test!(
    follows_a_default_re_export_of_a_svelte_component_629,
    "follows a default re-export of a .svelte component (export { default as Foo } from ./RealButton.svelte) (#629)"
);
ignored_backend_test!(
    links_an_astro_page_to_the_component_and_ts_util_it_uses_768,
    "links an .astro page to the component and TS util it uses (#768)"
);
ignored_backend_test!(
    resolves_a_bare_directory_import_to_index_ts_629,
    "resolves a bare directory import (import { x } from \".\" / \"./\") to index.ts (#629)"
);
ignored_backend_test!(
    resolves_a_workspace_package_subpath_barrel_to_its_index_629,
    "resolves a workspace package-subpath barrel (@scope/pkg/sub) to its index (#629)"
);
ignored_backend_test!(
    resolves_a_barrel_import_from_a_vue_sfc_script_block_629,
    "resolves a barrel import from a Vue SFC <script> block (#629)"
);
ignored_backend_test!(
    follows_a_vue_component_used_in_a_template_through_default_re_export_barrel_629,
    "follows a Vue component used in a <template> through a default re-export barrel (#629)"
);
