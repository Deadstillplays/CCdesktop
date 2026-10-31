use claude_desktop_rust::project::compiler::{ProjectCompiler, ProjectType};
use std::path::Path;

#[test]
fn test_detect_project_type_rust() {
    let p_type = ProjectCompiler::detect_project_type(Path::new("."));
    assert_eq!(p_type, ProjectType::RustCargo);
    assert_eq!(p_type.display_name(), "Rust (Cargo)");
}

#[test]
fn test_parse_rust_diagnostics() {
    let sample_output = r#"
   Compiling dummy v0.1.0
error[E0433]: failed to resolve: use of undeclared crate or module `foo`
 --> src/main.rs:12:5
   |
12 |     foo::bar();
   |     ^^^ use of undeclared crate or module `foo`

warning: unused variable: `x`
 --> src/lib.rs:8:9
  |
8 |     let x = 42;
  |         ^ help: if this is intentional, prefix it with an underscore: `_x`

error: aborting due to 1 previous error; 1 warning emitted
"#;

    let diags = ProjectCompiler::parse_diagnostics(&ProjectType::RustCargo, sample_output);
    assert_eq!(diags.len(), 2);

    let err = &diags[0];
    assert_eq!(err.severity, "error");
    assert_eq!(err.file.as_deref(), Some("src/main.rs"));
    assert_eq!(err.line, Some(12));
    assert_eq!(err.column, Some(5));
    assert_eq!(err.code.as_deref(), Some("E0433"));

    let warn = &diags[1];
    assert_eq!(warn.severity, "warning");
    assert_eq!(warn.file.as_deref(), Some("src/lib.rs"));
    assert_eq!(warn.line, Some(8));
    assert_eq!(warn.column, Some(9));
}

#[test]
fn test_parse_dotnet_diagnostics() {
    let sample_output = r#"
C:\Projects\MyApp\src\Program.cs(42,10): error CS0103: The name 'Console' does not exist in the current context [C:\Projects\MyApp\MyApp.csproj]
C:\Projects\MyApp\src\Utils.cs(15,8): warning CS0168: The variable 'ex' is declared but never used [C:\Projects\MyApp\MyApp.csproj]
"#;

    let diags = ProjectCompiler::parse_diagnostics(&ProjectType::DotNet, sample_output);
    assert_eq!(diags.len(), 2);

    let err = &diags[0];
    assert_eq!(err.severity, "error");
    assert!(err.file.as_deref().unwrap().ends_with("Program.cs"));
    assert_eq!(err.line, Some(42));
    assert_eq!(err.column, Some(10));
    assert_eq!(err.code.as_deref(), Some("CS0103"));

    let warn = &diags[1];
    assert_eq!(warn.severity, "warning");
    assert!(warn.file.as_deref().unwrap().ends_with("Utils.cs"));
    assert_eq!(warn.line, Some(15));
    assert_eq!(warn.column, Some(8));
    assert_eq!(warn.code.as_deref(), Some("CS0168"));
}

#[test]
fn test_parse_typescript_diagnostics() {
    let sample_output = r#"
src/components/App.tsx:25:7 - error TS2322: Type 'string' is not assignable to type 'number'.
src/index.ts:10:1 - warning TS6133: 'unusedVar' is declared but its value is never read.
"#;

    let diags = ProjectCompiler::parse_diagnostics(&ProjectType::TypeScriptNode, sample_output);
    assert_eq!(diags.len(), 2);

    let err = &diags[0];
    assert_eq!(err.severity, "error");
    assert_eq!(err.file.as_deref(), Some("src/components/App.tsx"));
    assert_eq!(err.line, Some(25));
    assert_eq!(err.column, Some(7));
    assert_eq!(err.code.as_deref(), Some("TS2322"));
}
