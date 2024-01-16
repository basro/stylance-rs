# Stylance changelog

## 0.0.11

-   Fixes cli watch mode not printing errors.
-   Removes unused features from tokio dependency to improve compilation times.

## 0.0.10

-   Added scss_prelude configuration option that lets you prefix text to the generated scss files.
-   Added debouncing to the stylance cli --watch mode.
-   Fixes an issue where stylance would read files while they were being modified by the text editor resulting in wrong output.

## 0.0.9

-   Added classes! utility macro for joining class names
-   Added JoinClasses trait

## 0.0.8

-   Renamed `output` config option to `output-file`
-   Added `output-dir` config option which generates one file per css module and an `_index.scss` file that imports all of them.
-   Improved `import_style!` and `import_crate_style!` proc macros error reporting.
-   Added support for style declarations inside of media queries (useful for SASS nested media queries)
-   Unknown fields in Cargo.toml `[package.metadata.stylance]` will now produce an error.
