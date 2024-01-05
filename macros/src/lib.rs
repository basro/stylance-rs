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
}

impl Parse for ImportStyleInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let ident: Ident = input.parse()?;
        input.parse::<Token![,]>()?;
        let string_literal: LitStr = input.parse()?;
        Ok(ImportStyleInput {
            style_ident: ident,
            style_path: string_literal.value(),
        })
    }
}

#[proc_macro]
pub fn import_style(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as ImportStyleInput);

    let manifest_dir_env = env::var_os("CARGO_MANIFEST_DIR").expect("we need CARGO_MANIFEST_DIR");
    let manifest_path = Path::new(&manifest_dir_env);
    let file_path = manifest_path.join(Path::new(&input.style_path));

    let (hash_str, classes) = core::get_classes(manifest_path, &file_path).expect("Load classes");

    let binding = file_path.canonicalize().unwrap();
    let full_path = binding.to_string_lossy();

    let identifiers = classes
        .iter()
        .map(|class| Ident::new(&class.original_name.replace('-', "_"), Span::call_site()))
        .collect::<Vec<_>>();

    let struct_ident = Ident::new(&format!("Style{hash_str}"), Span::call_site());

    let struct_definition = quote! {
        pub struct #struct_ident {
            #(pub #identifiers: &'static str,)*
            the_file: &'static str,
        }
    };

    // let output_fields = classes.iter().zip(identifiers).map(|(class, class_ident)| {
    //     let class_str = &class.hashed_name;
    //     quote! {
    //         #[deprecated]
    //         #class_ident: #class_str,
    //     }
    // });

    let output_fields = classes.iter().zip(identifiers).map(|(class, class_ident)| {
        let class_str = &class.hashed_name;
        quote! {
            #[allow(non_upper_case_globals)]
            pub const #class_ident: &str = #class_str;
        }
    });

    let style_ident = input.style_ident;

    quote! {
        const _ : &[u8] = include_bytes!(#full_path);

        pub mod #style_ident {
            #(#output_fields )*
        }
    }
    .into()
    // quote! {
    //     #struct_definition

    //     pub  const #style_ident: #struct_ident =
    //         #struct_ident {
    //             the_file: include_str!(#full_path),
    //             #(#output_fields )*
    //         }
    //     ;
    // }
}
