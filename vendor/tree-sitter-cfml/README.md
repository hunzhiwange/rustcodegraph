Native CFML tag grammar from cfmleditor/tree-sitter-cfml, commit
24051c091c9c1a69b36914533aee7aa3286d5526 (0.26.44), MIT.
C sources are unchanged apart from trailing whitespace cleanup in the script scanner. Tag and script grammars are wired for component/function declarations.
Query injections and call/import extraction are outside this integration.
