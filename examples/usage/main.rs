mod module;

use stylance::import_style;

use crate::module::style;

import_style!(style2, "examples/usage/style2.scss");

fn main() {
    import_style!(style3, "examples/usage/style1.scss");

    use style3 as stl;

    println!("{},{},{}", stl::foo, style2::baaa, style::bar)
}
