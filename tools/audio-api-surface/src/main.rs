//! Issue #153: existing rustdoc API extractor + narrow portable-source guard.
use std::{env, fs, process};
use syn::{Item, visit::Visit};

#[derive(Default)]
struct PortableSource {
    errors: Vec<&'static str>,
}

impl<'ast> Visit<'ast> for PortableSource {
    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        if attr.path().is_ident("doc") {
            return; // rustdoc includes hidden items too.
        }
        if attr.path().is_ident("derive") {
            let derives = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            );
            if derives.is_ok_and(|paths| {
                paths.iter().all(|path| {
                    ["Clone", "Copy", "Debug", "Default", "PartialEq", "Eq"]
                        .iter()
                        .any(|name| path.is_ident(name))
                })
            }) {
                return;
            }
        }
        self.errors.push("unadmitted attribute: portable contracts allow doc/built-in derives; conditional or expanding attributes require review");
    }

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if matches!(item.vis, syn::Visibility::Inherited)
            && item.attrs.iter().any(|attr| {
                attr.path().is_ident("cfg")
                    && attr
                        .parse_args::<syn::Path>()
                        .is_ok_and(|path| path.is_ident("test"))
            })
            && item
                .attrs
                .iter()
                .all(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("doc"))
        {
            return; // A private cfg(test) module cannot open production paths.
        }
        if item.content.is_none() {
            self.errors
                .push("unadmitted external module: only public ports is admitted");
        }
        syn::visit::visit_item_mod(self, item);
    }

    fn visit_expr_macro(&mut self, item: &'ast syn::ExprMacro) {
        // Nested macros/attributes in opaque macro tokens could introduce cfg
        // impls invisible on this host. Reject them, including inside groups.
        if opaque_expansion(&item.mac.tokens) {
            self.errors
                .push("unadmitted nested macro/attribute tokens in audio-api");
        }
        // Ordinary built-in expression macros cannot export crate contracts.
        if ![
            "assert",
            "assert_eq",
            "assert_ne",
            "debug_assert",
            "debug_assert_eq",
            "debug_assert_ne",
            "matches",
            "format",
            "vec",
            "panic",
            "unreachable",
            "todo",
            "println",
            "eprintln",
        ]
        .iter()
        .any(|name| item.mac.path.is_ident(name))
        {
            self.errors.push("unadmitted expression macro in audio-api");
        }
        for attr in &item.attrs {
            self.visit_attribute(attr);
        }
    }

    fn visit_stmt_macro(&mut self, item: &'ast syn::StmtMacro) {
        self.visit_expr_macro(&syn::ExprMacro {
            attrs: item.attrs.clone(),
            mac: item.mac.clone(),
        });
    }

    fn visit_macro(&mut self, _: &'ast syn::Macro) {
        self.errors
            .push("unadmitted item/type macro: generated contracts require review");
    }

    fn visit_item(&mut self, item: &'ast Item) {
        if matches!(item, Item::Use(item) if matches!(item.vis, syn::Visibility::Public(_))) {
            self.errors
                .push("unadmitted public re-export: the contract admits definitions in ports only");
        }
        if matches!(item, Item::Verbatim(_)) {
            self.errors.push("unsupported audio-api syntax");
        }
        syn::visit::visit_item(self, item);
    }
}

fn opaque_expansion(tokens: &proc_macro2::TokenStream) -> bool {
    tokens.clone().into_iter().any(|token| match token {
        proc_macro2::TokenTree::Group(group) => opaque_expansion(&group.stream()),
        proc_macro2::TokenTree::Punct(punct) => matches!(punct.as_char(), '#' | '!'),
        _ => false,
    })
}

fn extract() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().skip(1).collect();
    let mut guard = PortableSource::default();
    for path in &args[1..] {
        let parsed = syn::parse_file(&fs::read_to_string(path)?)?;
        for attr in &parsed.attrs {
            guard.visit_attribute(attr);
        }
        for item in &parsed.items {
            if std::path::Path::new(path)
                .file_name()
                .is_some_and(|name| name == "lib.rs")
                && let Item::Mod(module) = item
                && module.ident == "ports"
                && module.content.is_none()
                && matches!(module.vis, syn::Visibility::Public(_))
            {
                for attr in &module.attrs {
                    guard.visit_attribute(attr);
                }
                continue;
            }
            guard.visit_item(item);
        }
    }
    if !guard.errors.is_empty() {
        for error in guard.errors {
            eprintln!("AUDIO_API_SOURCE_POLICY_RED: {error}");
        }
        process::exit(1);
    }
    if args[0] == "--guard" {
        return Ok(());
    }
    let api = public_api::Builder::from_rustdoc_json(&args[0]).build()?;
    for item in api.items() {
        println!("{item}");
    }
    Ok(())
}

fn main() {
    if let Err(error) = extract() {
        eprintln!("TOOLING-FAIL: {error}");
        process::exit(2);
    }
}
