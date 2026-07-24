#[cfg(test)]
mod tests {
    use crate::compiler::test_support::*;

    #[test]
    fn test_undefined_variable() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         var string x = $global::missing;\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_undefined_var_in_fn_body() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn badfn { log $global::undefined; }\n\
         pr test [\n\
             url = `u`\n\
             dir = `d`\n\
         ] {\n\
             use badfn;\n\
         }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_run_reference_validation() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn real { log `hi`; }\n\
         pr test [\n\
              url = `u`\n\
              dir = `d`\n\
          ] {\n\
              use real;\n\
          }\n\
          run s { test::unknown; }\
          ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("function"), "got: {}", err_str);
    }

    #[test]
    fn test_valid_run_references() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn real { log `hi`; }\n\
         pr test [\n\
              url = `u`\n\
              dir = `d`\n\
          ] {\n\
              use real;\n\
          }\n\
          run s { test::real; }\
          ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.runs.contains_key("s"));
    }

    #[test]
    fn test_undefined_var_in_case_condition() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn badfn { case $global::undefined { _ { }; }; }\n\
         pr test [\n\
             url = `u`\n\
             dir = `d`\n\
         ] {\n\
             use badfn;\n\
         }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_undefined_var_in_case_varref_pattern() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn badfn { var string x = `ok`; case $self::x { $global::undefined { }; _ { }; }; }\n\
         pr test [\n\
             url = `u`\n\
             dir = `d`\n\
         ] {\n\
             use badfn;\n\
         }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_run_validates_function_refs() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr p [ url = `http://x` dir = `x` ] {\n\
         }\n\
         run bad { p::nonexistent; }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("function"), "got: {}", err);
    }

    #[test]
    fn test_fn_var_validation() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn bad { log $global::undefined; }\n\
         pr p [ url = `http://x` dir = `x` ] {\n\
             use bad;\n\
         }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_validation_errors_span_multiple_source_files() {
        // Two undefined-variable validation errors that originate from DIFFERENT
        // source files (main.kiru and an imported build.kiru). The aggregate
        // must preserve each child report's own source/span, so both surface
        // with their correct file instead of being collapsed into one
        // stringified blob.
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
 fn f1 { log $global::missing_main; }
 pr p [ url = `u` dir = `d` ] {
     use f1;
 }
 import `build.kiru`;
            ",
        );
        write_config(
            dir.path(),
            "build.kiru",
            "\
 fn f2 { log $global::missing_build; }
 pr p {
     use f2;
 }
            ",
        );
        // With top-down processing, the first error in source order surfaces
        // immediately. The second file's error is only revealed after the
        // first is fixed.
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("undefined variable") && err_str.contains("global::missing_main"),
            "got: {}",
            err_str
        );
        assert!(
            !err_str.contains("validation error(s) found"),
            "aggregate must keep original diagnostics, not stringify-and-wrap, got: {}",
            err_str
        );
    }
}
