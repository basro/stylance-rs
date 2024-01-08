mod module;

use stylance::import_style;

use module::style as module_style;

// This expands to
// mod my_style {
//     pub const contents: &str = "contents-f45126d";
//     pub const header: &str = "header-f45126d";
// }
import_style!(my_style, "examples/usage/style1.module.scss");

fn main() {
    println!(
        "my_style 'examples/usage/style1.module.scss' \nheader: {}",
        my_style::header
    );
    println!(
        "module_style 'examples/usage/style2.module.scss' \nheader: {}",
        module_style::header
    );
}
