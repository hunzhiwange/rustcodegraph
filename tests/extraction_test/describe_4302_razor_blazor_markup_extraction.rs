mod describe_4302_razor_blazor_markup_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Razor / Blazor markup extraction";
    const TS_DESCRIBE_LINE: usize = 4302;
    #[test]
    fn describes_061_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 4302);
    }
    #[test]
    fn case_4315_links_model_and_blazor_component_tags_to_their_c_types_ignores_html_el() {
        let suite = ["Razor / Blazor markup extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(231, 231);
        let temp = TempDir::new("codegraph-razor-model-component");
        temp.write(
            "LoginViewModel.cs",
            r#"namespace App { public class LoginViewModel { public string Email { get; set; } } }
"#,
        );
        temp.write(
            "ToastComponent.cs",
            r#"namespace App { public class ToastComponent { } }
"#,
        );
        temp.write(
            "Views/Login.cshtml",
            r#"@model LoginViewModel
<div class="form">
  <input asp-for="Email" />
</div>
"#,
        );
        temp.write(
            "Index.razor",
            r#"<div>
  <ToastComponent />
</div>
"#,
        );

        let mut cg = index_project(&temp);
        let vm = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "LoginViewModel")
            .expect("LoginViewModel should be indexed");
        let vm_deps = impact_file_paths(&mut cg, &vm.id, 2);
        assert!(
            vm_deps.iter().any(|path| path.ends_with("Login.cshtml")),
            "@model should link Login.cshtml: {vm_deps:?}"
        );
        let toast = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "ToastComponent")
            .expect("ToastComponent should be indexed");
        let toast_deps = impact_file_paths(&mut cg, &toast.id, 2);
        assert!(
            toast_deps.iter().any(|path| path.ends_with("Index.razor")),
            "Blazor tag should link Index.razor: {toast_deps:?}"
        );
        let html_nodes = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .filter(|node| node.name == "div" || node.name == "input")
            .collect::<Vec<_>>();
        assert!(html_nodes.is_empty(), "HTML nodes leaked: {html_nodes:?}");
        cg.close();
    }
    #[test]
    fn case_4355_c_namespaces_qualify_type_names_so_same_named_types_are_distinct() {
        let suite = ["Razor / Blazor markup extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(232, 232);
        let temp = TempDir::new("codegraph-csharp-qualified-names");
        temp.write(
            "entity.cs",
            "namespace App.Entities { public class CatalogBrand { } }\n",
        );
        temp.write(
            "dto.cs",
            "namespace App.Models { public class CatalogBrand { } }\n",
        );

        let mut cg = index_project(&temp);
        let brands = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .filter(|node| node.name == "CatalogBrand")
            .collect::<Vec<_>>();
        assert_eq!(brands.len(), 2, "brands: {brands:?}");
        let qns = brands
            .iter()
            .map(|node| node.qualified_name.clone())
            .collect::<Vec<_>>();
        assert_ne!(qns[0], qns[1]);
        assert!(qns.iter().any(|qn| qn == "App.Entities::CatalogBrand"));
        assert!(qns.iter().any(|qn| qn == "App.Models::CatalogBrand"));
        cg.close();
    }
    #[test]
    fn case_4370_disambiguates_a_razor_type_ref_via_using_incl_folder_imports_razor() {
        let suite = ["Razor / Blazor markup extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(233, 233);
        let temp = TempDir::new("codegraph-razor-using-disambiguation");
        temp.write(
            "Models/CatalogBrand.cs",
            "namespace App.Models { public class CatalogBrand { public int Id { get; set; } } }\n",
        );
        temp.write(
            "Entities/CatalogBrand.cs",
            "namespace App.Entities { public class CatalogBrand { public int Id { get; set; } } }\n",
        );
        temp.write("Pages/_Imports.razor", "@using App.Models\n");
        temp.write(
            "Pages/List.razor",
            r#"<h1>List</h1>
@code {
  private CatalogBrand _b = new CatalogBrand();
}
"#,
        );

        let mut cg = index_project(&temp);
        let dto = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.qualified_name == "App.Models::CatalogBrand")
            .expect("App.Models::CatalogBrand should be indexed");
        let entity = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.qualified_name == "App.Entities::CatalogBrand")
            .expect("App.Entities::CatalogBrand should be indexed");
        let dto_deps = impact_file_paths(&mut cg, &dto.id, 2);
        let entity_deps = impact_file_paths(&mut cg, &entity.id, 2);
        assert!(
            dto_deps.iter().any(|path| path.ends_with("List.razor")),
            "@using'd DTO should reach List.razor: {dto_deps:?}"
        );
        assert!(
            entity_deps.iter().all(|path| !path.ends_with("List.razor")),
            "same-named entity should not reach List.razor: {entity_deps:?}"
        );
        cg.close();
    }
    #[test]
    fn case_4398_delegates_blazor_code_block_c_to_cover_types_used_in_component_logic() {
        let suite = ["Razor / Blazor markup extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(234, 234);
        let temp = TempDir::new("codegraph-blazor-code-block");
        temp.write(
            "CatalogService.cs",
            "namespace App { public class CatalogService { public void Load() { } } }\n",
        );
        temp.write(
            "List.razor",
            r#"<h1>Catalog</h1>

@code {
  private CatalogService _svc = new CatalogService();
  void Refresh() { _svc.Load(); }
}
"#,
        );

        let mut cg = index_project(&temp);
        let svc = cg
            .get_nodes_by_kind(NodeKind::Class)
            .into_iter()
            .find(|node| node.name == "CatalogService")
            .expect("CatalogService should be indexed");
        let deps = impact_file_paths(&mut cg, &svc.id, 2);
        assert!(
            deps.iter().any(|path| path.ends_with("List.razor")),
            "@code usage should link List.razor: {deps:?}"
        );
        cg.close();
    }
}
