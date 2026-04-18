# Stylance-cli [![crates.io](https://img.shields.io/crates/v/stylance-cli.svg)](https://crates.io/crates/stylance-cli)

Stylance-cli is the build tool for [Stylance](https://github.com/basro/stylance-rs).

It reads your css module files and transforms them in the following way:

- Adds a hash as suffix to every classname found. (`.class` will become `.class-63gi2cY`)
- Removes any instance of `:global(contents)` while leaving contents intact.

For usage instructions see [Stylance](https://github.com/basro/stylance-rs) README.md.
