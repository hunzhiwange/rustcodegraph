mod describe_1326_swift_extraction {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Swift Extraction";
    const TS_DESCRIBE_LINE: usize = 1326;
    #[test]
    fn describes_014_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 1326);
    }
    #[test]
    fn case_1327_should_extract_class_declarations() {
        let suite = ["Swift Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(75, 75);
        let code = r#"
public class NetworkManager {
    private let session: URLSession

    public init(session: URLSession = .shared) {
        self.session = session
    }

    public func fetchData(from url: URL) async throws -> Data {
        let (data, _) = try await session.data(from: url)
        return data
    }
}
"#;
        let result = extract("NetworkManager.swift", code);
        let class_node = find_node(&result, NodeKind::Class, "NetworkManager")
            .expect("NetworkManager class should be extracted");
        assert_eq!(class_node.language, Language::Swift);
    }
    #[test]
    fn case_1349_should_extract_function_declarations() {
        let suite = ["Swift Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(76, 76);
        let code = r#"
func calculateSum(_ numbers: [Int]) -> Int {
    return numbers.reduce(0, +)
}

public func formatCurrency(amount: Double) -> String {
    return String(format: "$%.2f", amount)
}
"#;
        let result = extract("utils.swift", code);
        let functions = names_by_kind(&result, NodeKind::Function);
        assert_contains(&functions, "calculateSum");
        assert_contains(&functions, "formatCurrency");
    }
    #[test]
    fn case_1365_should_extract_struct_declarations() {
        let suite = ["Swift Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(77, 77);
        let code = r#"
public struct User {
    let id: UUID
    var name: String
    var email: String

    func displayName() -> String {
        return name
    }
}
"#;
        let result = extract("User.swift", code);
        let struct_node =
            find_node(&result, NodeKind::Struct, "User").expect("User struct should be extracted");
        assert_eq!(struct_node.language, Language::Swift);
    }
    #[test]
    fn case_1384_should_extract_protocol_declarations() {
        let suite = ["Swift Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(78, 78);
        let code = r#"
public protocol Repository {
    associatedtype Entity

    func find(id: String) async throws -> Entity?
    func save(_ entity: Entity) async throws
}
"#;
        let result = extract("Repository.swift", code);
        let protocol_node = find_node(&result, NodeKind::Interface, "Repository")
            .expect("Repository protocol should be extracted");
        assert_eq!(protocol_node.language, Language::Swift);
    }
    #[test]
    fn case_1400_should_extract_class_inheritance_and_protocol_conformance() {
        let suite = ["Swift Extraction"];
        assert_eq!(suite.len(), 1);
        assert_eq!(79, 79);
        let code = r#"
class DataRequest: Request {
    func validate() {}
}

class UploadRequest: DataRequest, Sendable {
    func upload() {}
}

enum AFError: Error {
    case invalidURL
}

struct HTTPMethod: RawRepresentable {
    let rawValue: String
}

protocol UploadConvertible: URLRequestConvertible {
    func asURLRequest() throws -> URLRequest
}
"#;
        let result = extract("Inheritance.swift", code);
        let extends = references_by_kind(&result, ReferenceKind::Extends);
        for name in [
            "Request",
            "DataRequest",
            "Sendable",
            "Error",
            "RawRepresentable",
            "URLRequestConvertible",
        ] {
            assert_contains(&extends, name);
        }
    }
}
