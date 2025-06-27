use stylance::*;

#[test]
fn test_import_crate_style() {
    import_crate_style!(style, "tests/style.module.scss");

    assert_eq!(style::style1, "style1-a331da9");
    assert_eq!(style::style2, "style2-a331da9");
    assert_eq!(style::style3, "style3-a331da9");
    assert_eq!(style::style4, "style4-a331da9");
    assert_eq!(style::style5, "style5-a331da9");
    assert_eq!(style::style6, "style6-a331da9");
    assert_eq!(style::style7, "style7-a331da9");
    assert_eq!(style::style8, "style8-a331da9");
    assert_eq!(style::style9, "style9-a331da9");

    assert_eq!(style::style_with_dashes, "style-with-dashes-a331da9");
    assert_eq!(style::nested_style, "nested-style-a331da9");

    mod some_module {
        stylance::import_crate_style!(#[allow(dead_code)] pub style, "tests/style.module.scss");
    }

    assert_eq!(some_module::style::style1, "style1-a331da9");

    import_crate_style!(style2, "tests/style2.module.scss");
    assert_eq!(style2::style1, "style1-58ea9e3");
    assert_eq!(style2::different_style, "different-style-58ea9e3");
}

#[test]
fn test_import_style() {
    import_style!(style, "style.module.scss");

    assert_eq!(style::style1, "style1-a331da9");
    assert_eq!(style::style2, "style2-a331da9");
    assert_eq!(style::style3, "style3-a331da9");
    assert_eq!(style::style4, "style4-a331da9");
    assert_eq!(style::style5, "style5-a331da9");
    assert_eq!(style::style6, "style6-a331da9");
    assert_eq!(style::style7, "style7-a331da9");
    assert_eq!(style::style8, "style8-a331da9");
    assert_eq!(style::style9, "style9-a331da9");

    assert_eq!(style::style_with_dashes, "style-with-dashes-a331da9");
    assert_eq!(style::nested_style, "nested-style-a331da9");

    mod some_module {
        stylance::import_style!(#[allow(dead_code)] pub style, "style.module.scss");
    }

    assert_eq!(some_module::style::style1, "style1-a331da9");

    import_style!(style2, "style2.module.scss");
    assert_eq!(style2::style1, "style1-58ea9e3");
    assert_eq!(style2::different_style, "different-style-58ea9e3");
}
