//#![feature(proc_macro_span)]

use std::{env, path::Path};

use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input, LitStr, Token,
};

struct ImportStyleInput {
    style_ident: Ident,
    style_path: String,
    is_pub: bool,
}

impl Parse for ImportStyleInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let is_pub = input.parse::<Token![pub]>().is_ok();
        let ident: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let string_literal: LitStr = input.parse()?;
        Ok(ImportStyleInput {
            style_ident: ident,
            style_path: string_literal.value(),
            is_pub,
        })
    }
}

#[proc_macro]
pub fn import_style_classes(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as LitStr);

    let manifest_dir_env = env::var_os("CARGO_MANIFEST_DIR").expect("we need CARGO_MANIFEST_DIR");
    let manifest_path = Path::new(&manifest_dir_env);
    let file_path = manifest_path.join(Path::new(&input.value()));

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
