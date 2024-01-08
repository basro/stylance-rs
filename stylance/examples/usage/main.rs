mod module;

use stylance::import_crate_style;

use module::style as module_style;

// This expands to
// mod my_style {
//     pub const contents: &str = "contents-f45126d";
//     pub const header: &str = "header-f45126d";
// }
import_crate_style!(
    my_style,
    "examples/usage/style1.module.scss" // Path is relative to the crate's Cargo.toml file
);

fn main() {
    println!(
        "my_style 'examples/usage/style1.module.scss' \nheader: {}",
        my_style::header
    );
    println!(
        "module_style 'examples/usage/style2.module.scss' \nheader: {}",
        module_style::header
    );

    // With the nightly feature you get access to import_style! which uses
    // paths relative to the rust file were it is called.
    // Requires rust nightly toolchain
    #[cfg(feature = "nightly")]
    {
        stylance::import_style!(rel_path_style, "style1.module.scss");
        println!(
            "rel_path_style 'style1.module.scss' \nheader: {}",
            rel_path_style::header
        );
    }
}
