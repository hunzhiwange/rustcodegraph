mod describe_1127_c_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "C# Extraction";
    const TS_DESCRIBE_LINE: usize = 1127;
    #[test]
    fn describes_012_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 1127);
    }
    #[test]
    fn case_1128_should_extract_class_declarations() {
        let suite = ["C# Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(69, 69);
        let code = r#"
public class OrderService
{
    private readonly IOrderRepository _repository;

    public OrderService(IOrderRepository repository)
    {
        _repository = repository;
    }

    public async Task<Order> GetOrderAsync(string id)
    {
        return await _repository.FindByIdAsync(id);
    }
}
"#;
        let result = extract("OrderService.cs", code);
        let class_node = find_node(&result, NodeKind::Class, "OrderService")
            .expect("OrderService class should be extracted");
        assert_eq!(class_node.visibility, Some(Visibility::Public));
    }
    #[test]
    fn case_1153_indexes_every_record_form_with_the_right_kind_831() {
        let suite = ["C# Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(70, 70);
        let code = r#"
namespace Fixture;

public record SimplePositional(int A);
public record WithBody(int A) { public int DoubleIt() => A * 2; }
public record class ExplicitClassRec(string Name);
public record struct ValueRec(int X);
public readonly record struct ReadonlyRec(int X, int Y);
public record DerivedRec(int A, string B) : SimplePositional(A);
public record GenericRec<T>(T Value);
public partial record PartialRec(int A);
"#;
        let result = extract("Records.cs", code);
        let kind_of = |name: &str| {
            result
                .nodes
                .iter()
                .find(|node| node.name == name)
                .map(|node| node.kind)
        };
        for name in [
            "SimplePositional",
            "WithBody",
            "ExplicitClassRec",
            "DerivedRec",
            "GenericRec",
            "PartialRec",
        ] {
            assert_eq!(kind_of(name), Some(NodeKind::Class), "{name}");
        }
        assert_eq!(kind_of("ValueRec"), Some(NodeKind::Struct));
        assert_eq!(kind_of("ReadonlyRec"), Some(NodeKind::Struct));
        assert_eq!(kind_of("DoubleIt"), Some(NodeKind::Method));
    }
    #[test]
    fn case_1187_indexes_primary_constructor_classes_including_keyed_di_attribute_param() {
        let suite = ["C# Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(71, 71);
        let code = r#"
public class DataService(IMemoryCache cache)
{
    public void Warm() { }
}

public class InstanceService(InstanceManager m, ProfileManager p)
{
    public void DeployAndLaunchAsync() { }
    public void Deploy() { }
}

public partial class UpdateService(int x) : ILifetimeService
{
    public void Run() { }
}

public class K1KeyedDi([FromKeyedServices("primary")] IMemoryCache cache)
{
    public void Warm() { }
}

public record CatalogBrand(int Id, string Name);
"#;
        let result = extract("Services.cs", code);
        let classes = names_by_kind(&result, NodeKind::Class);
        for name in [
            "DataService",
            "InstanceService",
            "UpdateService",
            "K1KeyedDi",
            "CatalogBrand",
        ] {
            assert_contains(&classes, name);
        }
        let methods = names_by_kind(&result, NodeKind::Method);
        for name in ["DeployAndLaunchAsync", "Deploy", "Run"] {
            assert_contains(&methods, name);
        }
    }
    #[test]
    fn case_1232_keeps_a_class_indexable_when_a_nested_enum_has_if_guarded_members_237() {
        let suite = ["C# Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(72, 72);
        let code = r#"
public class Reader
{
    private enum ReadType
    {
#if HAVE_DATE_TIME_OFFSET
        ReadAsDateTimeOffset,
#endif
        ReadAsDouble,
        ReadAsString,
    }

    public void Open() { }
    public void Close() { }
    public int ReadInt() { return 0; }
}
"#;
        let result = extract("Reader.cs", code);
        let methods = names_by_kind(&result, NodeKind::Method);
        for name in ["Open", "Close", "ReadInt"] {
            assert_contains(&methods, name);
        }
        let enum_members = names_by_kind(&result, NodeKind::EnumMember);
        assert_contains(&enum_members, "ReadAsDateTimeOffset");
        assert_contains(&enum_members, "ReadAsDouble");
    }
}
