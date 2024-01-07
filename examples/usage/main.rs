mod module;

use stylance::import_style;

use crate::module::style;

fn main() {
    import_style!(style3, "examples/usage/style1.module.scss");

    use style3 as stl;
}
