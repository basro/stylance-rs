# Stylance

Stylance is a library and tool for working with scoped CSS in rust inspired by CSS modules.

## Usage

Stylance is divided in two parts:

1. Rust proc macros for importing scoped/hashed class names from css files as constants into your rust code.
2. A cli tool that finds all css modules in your crate and generates an output css file with hashed class names.

### Proc macro

Add stylance to your rust cargo.toml:

```cli
cargo add stylance
```

Then use the import_crate_style proc read a css/scss file and bring the classes from within that file as constants.

css/scss file:

```css
/* src/component/card/card.module.scss */
.header {
  background-color: red;
}
```

rust file:

```rust
// src/component/card/card.rs

// Import a css file's classes:
stylance::import_crate_style!(my_style, "src/component/card/card.module.scss");

fn use_style() {
	// Use the classnames:
	println!("{}", my_style::header) // prints header-f45126d
}
```

All classnames found inside the file `src/component/card/card.module.scss` will be included as constants inside a module named as the identifier passed as first argument to import_style.

The proc macro has no other effects, generating the modified css file is done using the stylance cli.

### Stylance cli

Install stylance cli

```cli
cargo install stylance-cli
```

Run stylance cli:

```cli
stylance --output ./bundled.scss ./path/to/crate/dir/
```

This will find all the files ending with `.module.scss` and `.module.css`and bundle them into `./bundled.scss`, all classes will be modified to include a hash that matches the one the `import_crate_style!` macro produces.

Resulting output.scss:

```css
.header-f45126d {
  background-color: red;
}
```

During development it is convenient to use sylance cli in watch mode:

```
stylance --watch --output ./bundled.scss ./path/to/crate/dir/
```

The stylance process will then watch the `.module.css` files for changes and automatically rebuild the output file.
