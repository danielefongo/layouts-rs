use heck::ToSnakeCase;
use proc_macro2::{TokenStream as TokenStream2, TokenTree};
use quip::quip;
use quote::{ToTokens, format_ident};
use std::collections::HashSet;
use syn::{
    Error, FnArg, GenericArgument, Ident, Item, ItemFn, ItemMod, ItemUse, PathArguments, Result,
    ReturnType, Type, TypePath,
};

use crate::SpanError;

pub struct Stats {
    lets: StatLets,
    pub mod_uses: Vec<ItemUse>,
    pub sections: Vec<StatSection>,
}

impl Stats {
    pub fn used_metrics(&self) -> HashSet<String> {
        self.lets
            .used_metrics()
            .chain(self.sections.iter().flat_map(StatSection::used_metrics))
            .collect()
    }

    pub fn make_uses(&self) -> TokenStream2 {
        quip! { #(#{&self.mod_uses})* use crate::metrics::*; }
    }

    pub fn make_struct(&self) -> TokenStream2 {
        let stats_fields = self.sections.iter().map(|section| {
            let name = &section.name;
            quip! { pub #name: #name::Stats, }
        });
        let section_modules = self.sections.iter().map(|section| section.make_module());

        quip! {
            #{self.lets.make_struct()}
            #(#section_modules)*

            #[derive(Debug, Default, PartialEq)]
            pub struct Stats { #(#stats_fields)* }
        }
    }

    pub fn make_from_metrics(&self) -> TokenStream2 {
        let section_fields = self.sections.iter().map(|section| {
            let fields = section.stats.iter().map(|stat| {
                let args = stat.make_call_args();
                quip! { #{stat.name}: #{section.name}::#{stat.name}(#(#args)*), }
            });
            quip! { #{section.name}: #{section.name}::Stats { #(#fields)* }, }
        });

        quip! {
            impl From<crate::metrics::Metrics> for Stats {
                fn from(metrics: crate::metrics::Metrics) -> Self {
                    #{self.lets.make_let_create()}

                    Self {
                        #(#section_fields)*
                    }
                }
            }
        }
    }

    pub fn make_display(&self) -> TokenStream2 {
        let sections = self.sections.iter().map(StatSection::make_write_display);

        quip! {
            impl std::fmt::Display for Stats {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    #(#sections)*
                    Ok(())
                }
            }
        }
    }
}

impl TryFrom<ItemMod> for Stats {
    type Error = Error;

    fn try_from(module: ItemMod) -> Result<Self> {
        let (_, items) = module
            .content
            .ok_or(module.ident.error("Expected module to have content"))?;

        let mut mod_uses = Vec::new();
        let mut lets = Vec::new();
        let mut sections = Vec::new();

        for item in items {
            match item {
                Item::Use(use_item) => mod_uses.push(use_item),
                Item::Fn(func) => lets.push(StatLet::try_from(func)?),
                Item::Mod(section) => sections.push(StatSection::try_from(section)?),
                _ => return Err(item.error("Expected a #[stat_let] function or a section module")),
            }
        }

        Ok(Self {
            mod_uses,
            lets: StatLets { lets },
            sections,
        })
    }
}

struct StatLets {
    lets: Vec<StatLet>,
}

impl StatLets {
    fn make_struct(&self) -> TokenStream2 {
        let fields = self.lets.iter().map(|stat_let| {
            quip! { pub #{stat_let.name}: #{stat_let.kind}, }
        });

        let methods = self.lets.iter().map(|stat| {
            let mut method = stat.method.clone();
            method.sig.ident = stat.name.clone();
            method.attrs.clear();
            quip! { #method }
        });

        quip! {
            #[derive(Debug, Default, PartialEq)]
            pub struct StatLets { #(#fields)* }
            impl StatLets { #(#methods)* }
        }
    }

    fn make_let_create(&self) -> TokenStream2 {
        let fields = self.lets.iter().map(|stat| {
            let args = stat.make_call_args();
            quip! { #{&stat.name}: crate::stats::StatLets::#{stat.name}(#(#args)*), }
        });

        quip! {
            let lets = crate::stats::StatLets {
                #(#fields)*
            };
        }
    }

    fn used_metrics(&self) -> impl Iterator<Item = String> {
        self.lets.iter().flat_map(StatLet::used_metrics)
    }
}

struct StatLet {
    method: ItemFn,
    name: Ident,
    kind: StatKind,
    args: Vec<StatArg>,
}

impl StatLet {
    fn make_call_args(&self) -> impl Iterator<Item = TokenStream2> {
        self.args.iter().map(StatArg::make_call_arg)
    }

    fn used_metrics(&self) -> impl Iterator<Item = String> {
        self.args.iter().filter_map(StatArg::metric_name)
    }
}

impl TryFrom<ItemFn> for StatLet {
    type Error = Error;

    fn try_from(func: ItemFn) -> Result<Self> {
        if !func
            .attrs
            .iter()
            .any(|attr| attr.path().is_ident("stat_let"))
        {
            return Err(func
                .sig
                .ident
                .error("top-level functions must have #[stat_let]"));
        }

        let args = func
            .sig
            .inputs
            .iter()
            .map(StatArg::try_from)
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            method: func.clone(),
            name: func.sig.ident.clone(),
            kind: StatKind::try_from(&func.sig.output)?,
            args,
        })
    }
}

pub struct StatSection {
    pub mod_uses: Vec<ItemUse>,
    pub name: Ident,
    pub stats: Vec<Stat>,
}

impl StatSection {
    fn make_struct(&self) -> TokenStream2 {
        let fields = self
            .stats
            .iter()
            .map(|stat| quip! { pub #{stat.name}: #{stat.kind}, });

        quip! {
            #[derive(Debug, Default, PartialEq)]
            pub struct Stats { #(#fields)* }
        }
    }

    fn make_write_display(&self) -> TokenStream2 {
        let section_label = format!("{}:", self.name);
        let lines = self
            .stats
            .iter()
            .map(|stat| stat.make_write_display(&self.name));
        quip! { writeln!(f, #section_label)?; #(#lines)* }
    }

    fn make_module(&self) -> TokenStream2 {
        let methods = self.stats.iter().map(|stat| {
            let mut method = stat.method.clone();
            method.attrs.clear();
            quip! { pub #method }
        });

        let name = &self.name;
        let mod_uses = &self.mod_uses;
        quip! { pub mod #name { #(#mod_uses)* use super::StatLets; use crate::metrics::*; #{self.make_struct()} #(#methods)* } }
    }

    fn used_metrics(&self) -> impl Iterator<Item = String> {
        self.stats.iter().flat_map(Stat::used_metrics)
    }
}

impl TryFrom<ItemMod> for StatSection {
    type Error = Error;

    fn try_from(module: ItemMod) -> Result<Self> {
        let name = module.ident;

        let Some((_, items)) = module.content else {
            return Err(name.error("stats section should have content"));
        };

        let mut mod_uses = Vec::new();
        let mut stats = Vec::new();

        for item in items {
            match item {
                Item::Use(use_item) => mod_uses.push(use_item),
                Item::Fn(func) => stats.push(Stat::try_from(func)?),
                _ => return Err(item.error("Expected a function definition for a stat")),
            }
        }

        Ok(Self {
            name,
            stats,
            mod_uses,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum StatKind {
    Scalar,
    Map(Ident),
}

impl ToTokens for StatKind {
    fn to_tokens(&self, tokens: &mut TokenStream2) {
        tokens.extend(match self {
            Self::Scalar => quip! { f64 },
            Self::Map(key_ty) => quip! { crate::map::Map<#key_ty> },
        });
    }
}

impl TryFrom<&ReturnType> for StatKind {
    type Error = Error;

    fn try_from(return_type: &ReturnType) -> Result<Self> {
        let err = || return_type.error("Expected return type: f64 or Map<K>");

        let ReturnType::Type(_, ty) = return_type else {
            return Err(err());
        };

        match ty.as_ref() {
            Type::Path(TypePath { path, .. }) if path.is_ident("f64") => Ok(Self::Scalar),
            Type::Path(TypePath { path, .. }) => {
                let segment = path.segments.last().ok_or_else(err)?;
                if segment.ident != "Map" {
                    return Err(err());
                }

                let PathArguments::AngleBracketed(args) = &segment.arguments else {
                    return Err(err());
                };

                let Some(GenericArgument::Type(Type::Path(path))) = args.args.first() else {
                    return Err(err());
                };

                let ident = path
                    .path
                    .get_ident()
                    .ok_or_else(|| path.error("Expected a simple type identifier as map key"))?;

                Ok(Self::Map(ident.clone()))
            }
            _ => Err(err()),
        }
    }
}

pub struct Stat {
    pub method: ItemFn,
    pub name: Ident,
    pub kind: StatKind,
    pub target_name: Option<Ident>,
    args: Vec<StatArg>,
}

impl Stat {
    fn used_metrics(&self) -> impl Iterator<Item = String> {
        self.args.iter().filter_map(StatArg::metric_name)
    }

    fn make_call_args(&self) -> impl Iterator<Item = TokenStream2> {
        self.args.iter().map(StatArg::make_call_arg)
    }

    fn make_write_display(&self, section_name: &Ident) -> TokenStream2 {
        let name = &self.name;
        match &self.kind {
            StatKind::Map(_) => {
                let map_label = format!("  {}:", name);
                quip! {
                    writeln!(f, #map_label)?;
                    let mut entries: Vec<_> = self.#section_name.#name.entries().collect();
                    entries.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());
                    for (k, v) in entries { writeln!(f, "    {}: {:.2}", k, v)?; }
                }
            }
            StatKind::Scalar => {
                let label = format!("  {}: {{:.2}}", name);
                quip! { writeln!(f, #label, self.#section_name.#name)?; }
            }
        }
    }
}

impl TryFrom<ItemFn> for Stat {
    type Error = Error;

    fn try_from(func: ItemFn) -> Result<Self> {
        let args = func
            .sig
            .inputs
            .iter()
            .map(StatArg::try_from)
            .collect::<Result<Vec<_>>>()?;

        let target_name = target_attr_name(&func)?;
        Ok(Self {
            method: func.clone(),
            name: func.sig.ident.clone(),
            kind: StatKind::try_from(&func.sig.output)?,
            target_name,
            args,
        })
    }
}

enum StatArg {
    Lets,
    Metric(Ident),
}

impl StatArg {
    fn make_call_arg(&self) -> TokenStream2 {
        match self {
            Self::Lets => quip! { &lets, },
            Self::Metric(alias) => quip! { &metrics.#{metric_field_name(&alias)}, },
        }
    }

    fn metric_name(&self) -> Option<String> {
        match self {
            Self::Lets => None,
            Self::Metric(alias) => Some(alias.to_string()),
        }
    }
}

impl TryFrom<&FnArg> for StatArg {
    type Error = Error;

    fn try_from(arg: &FnArg) -> Result<Self> {
        let FnArg::Typed(pat_type) = arg else {
            return Err(arg.error("stat functions cannot take self"));
        };
        let Type::Reference(reference) = pat_type.ty.as_ref() else {
            return Err(pat_type
                .ty
                .error("stat function arguments must be references"));
        };
        let Type::Path(path) = reference.elem.as_ref() else {
            return Err(reference
                .elem
                .error("stat function argument type must be a simple path"));
        };
        let ident = path
            .path
            .get_ident()
            .ok_or_else(|| path.error("stat function argument type must be a simple identifier"))?
            .clone();

        if ident == "StatLets" {
            Ok(Self::Lets)
        } else {
            Ok(Self::Metric(ident))
        }
    }
}

fn metric_field_name(alias: &Ident) -> Ident {
    format_ident!("{}", alias.to_string().to_snake_case())
}

fn target_attr_name(method: &ItemFn) -> Result<Option<Ident>> {
    let mut found = None;
    for attr in &method.attrs {
        if !attr.path().is_ident("target") {
            continue;
        }
        if found.is_some() {
            return Err(attr.error("duplicate #[target(...)] attribute"));
        }

        let syn::Meta::List(list) = &attr.meta else {
            return Err(attr.error("expected #[target(name)]"));
        };
        let mut tokens = list.tokens.clone().into_iter();
        let Some(TokenTree::Ident(ident)) = tokens.next() else {
            return Err(list.error("expected #[target(name)]"));
        };
        if tokens.next().is_some() {
            return Err(list.error("expected #[target(name)]"));
        }

        found = Some(ident);
    }
    Ok(found)
}
