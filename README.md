# Stylance [![crates.io](https://img.shields.io/crates/v/stylance.svg)](https://crates.io/crates/stylance) ![tests](https://github.com/basro/stylance-rs/actions/workflows/tests.yml/badge.svg?branch=main)

Stylance is a library and cli tool for working with scoped CSS in rust.

**Features:**

- Import hashed class names from css files into your rust code as string constants. Trying to use a class name that doesn't exist in the css file becomes an error in rust.
- Bundle your css module files into a single output css file with all the class names transformed to include a hash (by using stylance cli).
- Class name hashes are deterministic and based on the relative path between the css file and your crate's manifest dir (where the Cargo.toml resides)
- CSS Bundle generation is independent of the rust build process, allowing for blazingly fast iteration when modifying the contents of a css style rule.

## Usage

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

### Accessing global classnames

Sometimes you might want to target classnames that are defined globally and outside of your css module. To do this you can wrap them with `:global()`

```css
.my_scoped_class :global(.paragraph) {
  color: red;
}
```

this will transform to:

```css
.my_scoped_class-f45126d .paragraph {
  color: red;
}
```

.my_scoped_class got the module hash attached but .paragraph was left alone while the `:global()` was removed.

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

Install stylance cli:

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

## Rust analyzer completion issues

### Nightly `import_style!`

Rust analyzer will not produce any completion for import_style!, this is because it doesn't support the nightly features used to obtain the current rust file path.

### Stable `import_crate_style!`

Rust analyzer will expand the `import_crate_style!(style, "src/mystyle.module.css")` macro properly the first time, which means you'll be able to get completion when typing `style::|`.

Unfortunately RA will cache the result and will not realize that it needs to reevaluate the proc macro when the contents of `src/mystyle.module.css` change.

This only affects completion, errors from cargo check will properly update.

The only way to force RA to reevaluate the macros is to restart the server or to rebuild all proc macros. Sadly this takes a really long time.

It is my opinion that no completion would be better than outdated completion.

Supposedly one should be able to disable the expansion of the macro by adding this to `.vscode/settings.json`

```json
"rust-analyzer.procMacro.ignored": {
   "stylance": ["import_style_classes"]
},
```

Unfortunately this doesn't seem to work at the moment, this rust analyzer feature might fix the issue: https://github.com/rust-lang/rust-analyzer/pull/15923

In the meantime the nightly `import_style` is my recommended way to work with this crate.
