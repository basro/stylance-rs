# Stylance changelog

## 0.5.0

-   Added support sass interpolation syntax in at rules and many other places.

## 0.4.0

-   Added support for @container at rules

## 0.3.0

-   Generated class name constants will now properly warn if they are unused.
-   Added attributes syntax to `import_style!` and `import_crate_style!` macros.

## 0.2.0

-   Add support for @layer at-rules
-   Made the order in which the modified css modules are output be well defined; Sorted by (filename, relativepath). This is important for rules with equal specificity or for cascade layers defined in the css modules.

## 0.1.1

-   Fixed the parser rejecting syntax of scss variable declarations (eg `$my-var: 10px;`).

## 0.1.0

-   Added `hash_len` configuration option that controls the length of the hash in generated class names.
-   Added `class_name_pattern` configuration option to control the generated class name pattern.
-   Added detection of hash collisions to stylance cli, it will error when detected. This allows reducing the hash_len without fear of it silently colliding.

## 0.0.12

-   Fixes cli watched folders not being relative to the manifest dir.

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
