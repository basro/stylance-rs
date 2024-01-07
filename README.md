# Stylance

Stylance is a tool and library for working with scoped CSS in rust.

## Usage

Stylance is divided in two parts:

1. A proc macro rust library for importing scoped classes from css modules as constants into your rust code.
2. A cli tool to generate a css file with the hashed classnames from css module files.

### Importing styles in rust using Proc Macros

Use the import_style proc macro to parse a css/scss file and bring the classes from within that file as constants inside a module.

```css
/* src/component/card/card.module.scss */
.header {
  background-color: red;
}
```

```rust
// src/component/card/card.rs

// Import a css file's classes:
stylance::import_style!(my_style, "src/component/card/card.module.scss");

fn use_style() {
	// Use the classnames:
	println!("{}", my_style::header) // prints header-f45126d
}
```

All classnames found inside the file will be included as constants inside a module named as the identifier passed as first argument to import_style.

The proc macro has no other effects, generating the modified css file is done using the stylance-cli.
