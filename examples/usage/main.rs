mod module;

use stylance::import_style;

use crate::module::style;

import_style!(style1, "examples/usage/style1.scss");

fn main() {
    style1::bar;
    style1::foo;
}