use proc_macro2::{Group, TokenStream, TokenTree};
use quote::{quote, TokenStreamExt};
use syn::{
    braced, parse::Parse, Attribute, Block, FnArg, Generics, Ident, Pat, Stmt, Token, TraitItem,
    Visibility,
};

pub struct Input {
    attrs: Vec<Attribute>,
    vis: Visibility,
    trait_token: Token![trait],
    name: Ident,
    mut_name: Ident,
    generics: Generics,
    items: TokenStream,
}

impl Parse for Input {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis = input.parse()?;
        let trait_token = input.parse()?;
        let name: Ident = input.parse()?;
        let mut_name: Ident = input.parse()?;
        let generics: Generics = input.parse()?;

        let content;
        braced!(content in input);
        let items = content.parse()?;

        Ok(Self { attrs, vis, trait_token, name, mut_name, generics, items })
    }
}

impl Input {
    pub fn expand(&self) -> TokenStream {
        let Self { attrs, vis, trait_token, name, mut_name, generics, items } = self;

        let (nonmut_items, mut_items) = expand_streams(items);
        // Better IDE support.
        let fallback = || {
            quote! {
                #(#attrs)*
                #vis #trait_token #name #generics {
                    #nonmut_items
                }

                #(#attrs)*
                #vis #trait_token #mut_name #generics {
                    #mut_items
                }
            }
        };
        let Ok(mut nonmut_trait_items) = parse_trait_items(nonmut_items.clone()) else {
            return fallback();
        };
        let Ok(mut mut_trait_items) = parse_trait_items(mut_items.clone()) else {
            return fallback();
        };

        for item in &mut mut_trait_items {
            if let TraitItem::Fn(f) = item {
                f.sig.ident = Ident::new(&format!("{}_mut", f.sig.ident), f.sig.ident.span());
            }
        }

        add_walk_fns(&mut mut_trait_items);
        add_walk_fns(&mut nonmut_trait_items);

        quote! {
            #(#attrs)*
            #vis #trait_token #name #generics {
                #(#nonmut_trait_items)*
            }

            #(#attrs)*
            #vis #trait_token #mut_name #generics {
                #(#mut_trait_items)*
            }
        }
    }
}

// (nonmut, mut)
// nonmut skips `#mut` and mut includes `#mut` as `mut`
fn expand_streams(tts: &TokenStream) -> (TokenStream, TokenStream) {
    let mut nonmut_tts = TokenStream::new();
    let mut mut_tts = TokenStream::new();
    let mut tt_iter = tts.clone().into_iter();
    while let Some(tt) = tt_iter.next() {
        match tt {
            TokenTree::Group(group) => {
                let (nm, m) = expand_streams(&group.stream());
                nonmut_tts.append(Group::new(group.delimiter(), nm));
                mut_tts.append(Group::new(group.delimiter(), m));
            }
            TokenTree::Punct(punct)
                if punct.as_char() == '#' && tt_iter.clone().next().is_some_and(is_token_mut) =>
            {
                let mut_token = tt_iter.next().unwrap();
                mut_tts.append(mut_token);
            }
            TokenTree::Ident(id)
                if tt_iter.clone().next().is_some_and(is_token_hash)
                    && tt_iter.clone().nth(1).is_some_and(is_token_underscore_mut) =>
            {
                let _ = tt_iter.next();
                let _ = tt_iter.next();
                mut_tts.append(Ident::new(&format!("{id}_mut"), id.span()));
                nonmut_tts.append(id);
            }
            tt => {
                nonmut_tts.append(tt.clone());
                mut_tts.append(tt);
            }
        }
    }
    (nonmut_tts, mut_tts)
}

fn is_token_hash(tt: TokenTree) -> bool {
    if let TokenTree::Punct(punct) = tt {
        return punct.as_char() == '#';
    }
    false
}

fn is_token_mut(tt: TokenTree) -> bool {
    if let TokenTree::Ident(ident) = tt {
        return ident == "mut";
    }
    false
}

fn is_token_underscore_mut(tt: TokenTree) -> bool {
    if let TokenTree::Ident(ident) = tt {
        return ident == "_mut";
    }
    false
}

fn parse_trait_items(tts: TokenStream) -> Result<Vec<TraitItem>, syn::Error> {
    struct TraitItems(Vec<TraitItem>);
    impl Parse for TraitItems {
        fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
            let mut items = vec![];
            while !input.is_empty() {
                items.push(input.parse()?);
            }
            Ok(Self(items))
        }
    }
    Ok(syn::parse2::<TraitItems>(tts)?.0)
}

// fn visit_... { stmts @ ... } -> fn visit_... { self.walk_...(...) }
// + fn walk_... { #stmts }
fn add_walk_fns(items: &mut Vec<TraitItem>) {
    for i in 0..items.len() {
        let item = &mut items[i];
        if let TraitItem::Fn(f) = item {
            let name = f.sig.ident.to_string();
            let Some(name) = name.strip_prefix("visit_") else { continue };
            let walk_name = Ident::new(&format!("walk_{name}"), f.sig.ident.span());

            let mut walk_fn = f.clone();
            let Some(body) = &mut f.default else { continue };
            f.attrs.push(syn::parse_quote!(#[inline]));

            let args = f.sig.inputs.iter().filter_map(|arg| {
                Some(match arg {
                    FnArg::Receiver(_rec) => return None,
                    FnArg::Typed(pat) => match &*pat.pat {
                        Pat::Ident(ident) => {
                            let id = &ident.ident;
                            quote!(#id)
                        }
                        _ => return None,
                    },
                })
            });
            let call_walk = syn::parse_quote! {
                self.#walk_name(#(#args),*)
            };
            let call_walk_stmt = Stmt::Expr(call_walk, None);
            let walk_stmts = std::mem::replace(&mut body.stmts, vec![call_walk_stmt]);

            walk_fn.sig.ident = walk_name;
            walk_fn.default =
                Some(Block { brace_token: body.brace_token.clone(), stmts: walk_stmts });
            items.push(TraitItem::Fn(walk_fn));
        }
    }
}
