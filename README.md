# Stylance ![crates.io](https://img.shields.io/crates/v/stylance.svg)

Stylance is a library and cli tool for working with scoped CSS in rust.

# Usage

Stylance is divided in two parts:

1. Rust proc macros for importing scoped class names from css files as string constants into your rust code.
2. A cli tool that finds all css modules in your crate and generates an output css file with hashed class names.

## Proc macro

Add stylance as a dependency:

```cli
cargo add stylance
```

Then use the import_crate_style proc macro to read a css/scss file and bring the classes from within that file as constants.

`src/component/card/card.module.scss` file's content:

```css
/*  */
.header {
  background-color: red;
}
```

`src/component/card/card.rs` file's contents:

```rust
// Import a css file's classes:
stylance::import_crate_style!(my_style, "src/component/card/card.module.scss");

fn use_style() {
	// Use the classnames:
	println!("{}", my_style::header) // prints header-f45126d
}
```

All class names found inside the file `src/component/card/card.module.scss` will be included as constants inside a module named as the identifier passed as first argument to import_style.

The proc macro has no side effects, to generate the transformed css file we then use the stylance cli.

### Nightly feature

If you are using rust nightly you can enable the `nightly` feature to get access to the `import_style!` macro which lets you specify the css module file as relative to the current file.

Enable the nightly feature:

```toml
stylance = { version = "<version here>", features = ["nightly"] }
```

Then import style as relative:

`src/component/card/card.rs`:

```rust
stylance::import_style!(my_style, "card.module.scss");
```

## Stylance cli

Install stylance cli

```cli
cargo install stylance-cli
```

Run stylance cli:

```cli
stylance ./path/to/crate/dir/ --output ./bundled.scss
```

The first argument is the path to the directory containing the Cargo.toml of your package/crate.

This will find all the files ending with `.module.scss` and `.module.css`and bundle them into `./bundled.scss`, all classes will be modified to include a hash that matches the one the `import_crate_style!` macro produces.

Resulting `./bundled.scss`:

```css
.header-f45126d {
  background-color: red;
}
```

By default stylance cli will only look for css modules inside the crate's `./src/` folder. This can be [configured](#configuration).

### Watching for changes

During development it is convenient to use sylance cli in watch mode:

```cli
stylance --watch --output ./bundled.scss ./path/to/crate/dir/
```

The stylance process will then watch any `.module.css` and `.module.scss` files for changes and automatically rebuild the output file.

## <a name="configuration"></a> Configuration

Stylance configuration lives inside the Cargo.toml file of your crate.

All configuration settings are optional.

```toml
[package.metadata.stylance]

# output
# output file to generate
# has no default value, when not set you must provide an output
# to the stylance cli using the --output argumnent.
output = "./styles/bundle.scss"

# folders
# folders in which stylance cli will look for css module files.
# defaults to ["./src/"]
folders = ["./src/", "./styles/"]

# extensions
# files ending with these extensions will be considered to be
# css modules by stylance cli and will be included in the output
# bundle
# defaults to [".module.scss", ".module.css"]
extensions = [".module.scss", ".module.css"]
```
