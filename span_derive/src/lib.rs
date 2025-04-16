use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_derive(Span)]
pub fn derive_span(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident.clone();
    let generics = input.generics.clone();

    let expanded = match input.data {
        Data::Struct(ref data_struct) => {
            let fields = match &data_struct.fields {
                Fields::Named(fields) => &fields.named,
                _ => {
                    return syn::Error::new_spanned(
                        data_struct.fields.to_token_stream(),
                        "Span can only be derived for structs with named fields",
                    )
                    .to_compile_error()
                    .into();
                }
            };

            // Ensure the `span` field exists
            let has_span_field = fields.iter().any(|field| {
                field
                    .ident
                    .as_ref()
                    .map(|ident| ident == "span")
                    .unwrap_or(false)
            });

            if !has_span_field {
                return syn::Error::new_spanned(
                    data_struct.fields.to_token_stream(),
                    "Struct must have a `span` field of type `miette::SourceSpan` to derive `Span`",
                )
                .to_compile_error()
                .into();
            }

            // Generate the `HasSpan` implementation
            let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
            quote! {
            impl #impl_generics #name #ty_generics #where_clause {
                    pub fn label(&self, text: impl Into<String>) -> miette::LabeledSpan {
                        miette::LabeledSpan::at(self.span, text.into())
                    }

                    pub fn with_span(mut self, span: miette::SourceSpan) -> Self {
                        self.span = span;
                        self
                    }
                }
            }
        }
        _ => {
            return syn::Error::new_spanned(input.clone(), "Span can only be derived for structs")
                .to_compile_error()
                .into();
        }
    };

    TokenStream::from(expanded)
}
