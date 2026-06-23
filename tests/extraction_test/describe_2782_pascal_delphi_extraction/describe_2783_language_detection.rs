mod describe_2783_language_detection {
    use super::*;
    const TS_DESCRIBE_TITLE: &str = "Language detection";
    const TS_DESCRIBE_LINE: usize = 2783;
    #[test]
    fn describes_035_is_represented() {
        assert!(!TS_DESCRIBE_TITLE.is_empty());
        assert_eq!(TS_DESCRIBE_LINE, 2783);
    }
    #[test]
    fn case_2784_should_detect_pascal_files() {
        let suite = ["Pascal / Delphi Extraction", "Language detection"];
        assert_eq!(suite.len(), 2);
        assert_eq!(179, 179);
        assert_detected_language("UAuth.pas", None, Language::Pascal);
        assert_detected_language("App.dpr", None, Language::Pascal);
        assert_detected_language("Package.dpk", None, Language::Pascal);
        assert_detected_language("App.lpr", None, Language::Pascal);
        assert_detected_language("MainForm.dfm", None, Language::Pascal);
        assert_detected_language("MainForm.fmx", None, Language::Pascal);
    }
    #[test]
    fn case_2793_should_report_pascal_as_supported() {
        let suite = ["Pascal / Delphi Extraction", "Language detection"];
        assert_eq!(suite.len(), 2);
        assert_eq!(180, 180);
        assert_language_support(Language::Pascal, true);
        assert_supported_languages_include(&[Language::Pascal]);
    }
}
