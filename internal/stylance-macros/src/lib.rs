#![cfg_attr(nightly, feature(proc_macro_span))]
#![feature(proc_macro_span)]

use std::{env, path::Path};

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn import_style_classes(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);

    let manifest_dir_env = env::var_os("CARGO_MANIFEST_DIR").expect("we need CARGO_MANIFEST_DIR");
    let manifest_path = Path::new(&manifest_dir_env);
    let file_path = manifest_path.join(Path::new(&input.value()));

    let (_, classes) = stylance_core::get_classes(manifest_path, &file_path).expect("Load classes");

    let binding = file_path.canonicalize().unwrap();
    let full_path = binding.to_string_lossy();

    let identifiers = classes
        .iter()
        .map(|class| Ident::new(&class.original_name.replace('-', "_"), Span::call_site()))
        .collect::<Vec<_>>();

    let output_fields = classes.iter().zip(identifiers).map(|(class, class_ident)| {
        let class_str = &class.hashed_name;
        quote! {
            #[allow(non_upper_case_globals)]
            pub const #class_ident: &str = #class_str;
        }
    });

    quote! {
        const _ : &[u8] = include_bytes!(#full_path);
        #(#output_fields )*
    }
    .into()
}

#[cfg(feature = "nightly")]
#[proc_macro]
pub fn import_style_classes_rel(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);

    let manifest_dir_env = env::var_os("CARGO_MANIFEST_DIR").expect("we need CARGO_MANIFEST_DIR");
    let manifest_path = Path::new(&manifest_dir_env);

    let call_site_path = proc_macro::Span::call_site().source().source_file().path();
    let file_path = call_site_path
        .parent()
        .expect("No current path")
        .join(input.value());

    let (_, classes) = core::get_classes(manifest_path, &file_path).expect("Load classes");

    let binding = file_path.canonicalize().unwrap();
    let full_path = binding.to_string_lossy();

    let identifiers = classes
        .iter()
        .map(|class| Ident::new(&class.original_name.replace('-', "_"), Span::call_site()))
        .collect::<Vec<_>>();

    let output_fields = classes.iter().zip(identifiers).map(|(class, class_ident)| {
        let class_str = &class.hashed_name;
        quote! {
            #[allow(non_upper_case_globals)]
            pub const #class_ident: &str = #class_str;
        }
    });

    quote! {
        const _ : &[u8] = include_bytes!(#full_path);
        #(#output_fields )*
    }
    .into()
}
