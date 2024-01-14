# Stylance changelog

## 0.0.9

- Added JoinClasses trait
- Added classes! utility macro

## 0.0.8

- Renamed `output` config option to `output-file`
- Added `output-dir` config option which generates one file per css module and an `_index.scss` file that imports all of them.
- Improved `import_style!` and `import_crate_style!` proc macros error reporting.
- Added support for style declarations inside of media queries (useful for SASS nested media queries)
- Unknown fields in Cargo.toml `[package.metadata.stylance]` will now produce an error.
