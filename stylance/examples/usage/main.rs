mod module;

use stylance::{classes, import_crate_style};

use module::style as module_style;

import_crate_style!(my_style, "examples/usage/style1.module.scss");

fn main() {
    println!(
        "my_style 'examples/usage/style1.module.scss' \nheader: {}",
        my_style::header
    );
    println!(
        "module_style 'examples/usage/style2.module.scss' \nheader: {}",
        module_style::header
    );

    // Easily combine two or more classes using the classes! macro
    let active_tab = 0; // set to 1 to disable the active class!
    println!(
        "The two classes combined: '{}'",
        classes!(
            "some-global-class",
            my_style::header,
            my_style::contents,
            module_style::header,
            (active_tab == 0).then_some(my_style::active) // conditionally activate a global style
        ),
    );

    // With the nightly feature you get access to import_style! which uses
    // paths relative to the rust file were it is called.
    // Requires rust nightly toolchain
    #[cfg(feature = "nightly")]
    {
        stylance::import_style!(#[allow(dead_code)] rel_path_style, "style1.module.scss");
        println!(
            "rel_path_style 'style1.module.scss' \nheader: {}",
            rel_path_style::header
        );
    }
}
