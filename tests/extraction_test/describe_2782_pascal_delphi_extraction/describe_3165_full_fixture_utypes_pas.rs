mod describe_3165_full_fixture_utypes_pas {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Full fixture: UTypes.pas";
    const TS_DESCRIBE_LINE: usize = 3165;
    const CODE: &str = r#"unit UTypes;

interface

uses
  System.SysUtils;

const
  C_MAX_RETRIES = 3;
  C_DEFAULT_NAME = 'Guest';

type
  TUserRole = (urAdmin, urEditor, urViewer);

  TPoint2D = record
    X: Double;
    Y: Double;
  end;

  TUserName = string;

  TUserInfo = class
  public
    type
      TAddress = record
        Street: string;
        City: string;
        Zip: string;
      end;
  private
    FName: TUserName;
    FRole: TUserRole;
    FAddress: TAddress;
  public
    constructor Create(const AName: TUserName; ARole: TUserRole);
    function GetDisplayName: string;
    class function CreateAdmin(const AName: TUserName): TUserInfo; static;
    property Name: TUserName read FName write FName;
    property Role: TUserRole read FRole;
    property Address: TAddress read FAddress write FAddress;
  end;

implementation

constructor TUserInfo.Create(const AName: TUserName; ARole: TUserRole);
begin
  FName := AName;
  FRole := ARole;
end;

function TUserInfo.GetDisplayName: string;
begin
  if FRole = urAdmin then
    Result := '[Admin] ' + FName
  else
    Result := FName;
end;

class function TUserInfo.CreateAdmin(const AName: TUserName): TUserInfo;
begin
  Result := TUserInfo.Create(AName, urAdmin);
end;

end."#;

    #[test]
    fn describes_049_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 3165);
    }
    #[test]
    fn case_3231_should_extract_enums_with_members() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UTypes.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(202, 202);
        let result = extract("UTypes.pas", CODE);
        find_node(&result, NodeKind::Enum, "TUserRole").expect("TUserRole enum should exist");
        assert_eq!(
            names_by_kind(&result, NodeKind::EnumMember),
            ["urAdmin", "urEditor", "urViewer"]
        );
    }
    #[test]
    fn case_3242_should_extract_constants() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UTypes.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(203, 203);
        let result = extract("UTypes.pas", CODE);
        let constants = names_by_kind(&result, NodeKind::Constant);
        assert_eq!(constants.len(), 2, "constants: {constants:?}");
        assert_contains(&constants, "C_MAX_RETRIES");
        assert_contains(&constants, "C_DEFAULT_NAME");
    }
    #[test]
    fn case_3251_should_extract_type_aliases() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UTypes.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(204, 204);
        let result = extract("UTypes.pas", CODE);
        assert_contains(&names_by_kind(&result, NodeKind::TypeAlias), "TUserName");
    }
    #[test]
    fn case_3258_should_extract_records_as_classes_with_fields() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UTypes.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(205, 205);
        let result = extract("UTypes.pas", CODE);
        assert_contains(&names_by_kind(&result, NodeKind::Class), "TPoint2D");
        let fields = names_by_kind(&result, NodeKind::Field);
        assert_contains(&fields, "X");
        assert_contains(&fields, "Y");
    }
    #[test]
    fn case_3270_should_extract_static_class_methods() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UTypes.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(206, 206);
        let result = extract("UTypes.pas", CODE);
        let method = find_node(&result, NodeKind::Method, "CreateAdmin")
            .expect("CreateAdmin should be extracted");
        assert_eq!(method.is_static, Some(true));
    }
    #[test]
    fn case_3279_should_extract_nested_types() {
        let suite = ["Pascal / Delphi Extraction", "Full fixture: UTypes.pas"];
        assert_eq!(suite.len(), 2);
        assert_eq!(207, 207);
        let result = extract("UTypes.pas", CODE);
        assert_contains(&names_by_kind(&result, NodeKind::Class), "TAddress");
    }
}
