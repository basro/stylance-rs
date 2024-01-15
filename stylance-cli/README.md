# Stylance-cli [![crates.io](https://img.shields.io/crates/v/stylance-cli.svg)](https://crates.io/crates/stylance-cli)

Stylance-cli is the build tool for [Stylance](https://github.com/basro/stylance-rs).

It reads your css module files and transforms them in the following way:

- Adds a hash as suffix to every classname found. (`.class` will become `.class-63gi2cY`)
- Removes any instance of `:global(contents)` while leaving contents intact.

## Installation

Install stylance cli:

```cli
cargo install stylance-cli
```

## Usage

Run stylance cli:

```cli
stylance ./path/to/crate/dir/ --output-file ./bundled.scss
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

### <a name="SASS"></a> Use `output-dir` for better SASS compatibility

If you plan to use the output of stylance in a SASS project (by importing it from a .scss file), then I recommend using the `output-dir` option instead of `output-file`.

```bash
stylance ./path/to/crate/dir/ --output-dir ./styles/
```

This will create the folder `./styles/stylance/`.

When using --output-dir (or output_dir in package.metadata.stylance) stylance will not bundle the transformed module files, instead it will create a "stylance" folder in the specified output-dir path which will contain all the transformed css modules inside as individual files.

This "stylance" folder also includes an \_index.scss file that imports all the transformed scss modules.

You can then use `@use "path/to/the/folder/stylance"` to import the css modules into your sass project.

### Watching for changes

During development it is convenient to use sylance cli in watch mode:

```cli
stylance --watch --output-file ./bundled.scss ./path/to/crate/dir/
```

The stylance process will then watch any `.module.css` and `.module.scss` files for changes and automatically rebuild the output file.

## <a name="configuration"></a> Configuration

Stylance configuration lives inside the Cargo.toml file of your crate.

All configuration settings are optional.

```toml
[package.metadata.stylance]

# output_file
# When set, stylance-cli will bundle all css module files
# into by concatenating them and put the result in this file.
output_file = "./styles/bundle.scss"

# output_dir
# When set, stylance-cli will create a folder named "stylance" inside
# the output_dir directory.
# The stylance folder will be populated with one file per detected css module
# and one _all.scss file that contains one `@use "file.module-hash.scss";` statement
# per module file.
# You can use that file to import all your modules into your main scss project.
output_dir = "./styles/"

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

# scss_prelude
# When generating an scss file stylance-cli will prepend this string
# Useful to include a @use statement to all scss modules.
scss_prelude = '@use "../path/to/prelude" as *;'
```
