mod describe_3025_full_fixture_uauth_pas {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Full fixture: UAuth.pas";
    const TS_DESCRIBE_LINE: usize = 3025;
    const CODE: &str = r#"unit UAuth;

interface

uses
  System.SysUtils,
  System.Classes;

type
  ITokenValidator = interface
    ['{11111111-1111-1111-1111-111111111111}']
    function Validate(const AToken: string): Boolean;
  end;

  TAuthService = class(TInterfacedObject, ITokenValidator)
  private
    FToken: string;
    FLoginCount: Integer;
    procedure IncLoginCount;
  protected
    function GetToken: string;
  public
    constructor Create;
    destructor Destroy; override;
    function Validate(const AToken: string): Boolean;
    function Login(const AUser, APass: string): string;
    property Token: string read GetToken;
    property LoginCount: Integer read FLoginCount;
  end;

implementation

constructor TAuthService.Create;
begin
  inherited Create;
  FToken := '';
  FLoginCount := 0;
end;

destructor TAuthService.Destroy;
begin
  FToken := '';
  inherited Destroy;
end;

procedure TAuthService.IncLoginCount;
begin
  Inc(FLoginCount);
end;

function TAuthService.GetToken: string;
begin
  Result := FToken;
end;

function TAuthService.Validate(const AToken: string): Boolean;
begin
  Result := AToken <> '';
end;

function TAuthService.Login(const AUser, APass: string): string;
begin
  IncLoginCount;
  if Validate(AUser + ':' + APass) then
  begin
    FToken := AUser;
    Result := 'ok';
  end
  else
    Result := '';
end;

end."#;

    #[test]
    fn describes_048_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3025);
    }
    #[test]
    fn case_3100_should_extract_all_expected_nodes() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UAuth.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(199, 199);
        let result = extract("UAuth.pas", CODE);
        assert!(result.errors.is_empty(), "errors: {:?}", result.errors);
        find_node(&result, NodeKind::Module, "UAuth").expect("module should exist");
        assert_eq!(nodes_by_kind(&result, NodeKind::Import).len(), 2);
        find_node(&result, NodeKind::Interface, "ITokenValidator").expect("interface should exist");
        find_node(&result, NodeKind::Class, "TAuthService").expect("class should exist");

        let methods = names_by_kind(&result, NodeKind::Method);
        assert!(
            methods.len() >= 6,
            "expected at least 6 methods, got {methods:?}"
        );
        assert_contains(&methods, "Create");
        assert_contains(&methods, "Destroy");
        assert_contains(&methods, "Login");

        let fields = nodes_by_kind(&result, NodeKind::Field);
        assert_eq!(fields.len(), 2, "fields: {fields:?}");
        assert!(fields
            .iter()
            .all(|field| field.visibility == Some(Visibility::Private)));

        let props = names_by_kind(&result, NodeKind::Property);
        assert_eq!(props.len(), 2, "properties: {props:?}");
        assert_contains(&props, "Token");
        assert_contains(&props, "LoginCount");
    }
    #[test]
    fn case_3140_should_extract_inheritance_and_interface_implementation() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UAuth.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(200, 200);
        let result = extract("UAuth.pas", CODE);
        assert_contains(
            &references_by_kind(&result, ReferenceKind::Extends),
            "TInterfacedObject",
        );
        assert_contains(
            &references_by_kind(&result, ReferenceKind::Implements),
            "ITokenValidator",
        );
    }
    #[test]
    fn case_3154_should_extract_calls_from_implementation() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UAuth.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(201, 201);
        let result = extract("UAuth.pas", CODE);
        let calls = references_by_kind(&result, ReferenceKind::Calls);
        assert_contains(&calls, "Inc");
        assert_contains(&calls, "Validate");
    }
}
