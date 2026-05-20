use heck::ToPascalCase;
use proc_macro2::TokenStream as TokenStream2;
use quip::quip;
use quote::{ToTokens, format_ident};
use std::collections::HashSet;
use syn::{
    Error, FnArg, GenericArgument, Ident, Item, ItemFn, ItemMod, ItemUse, PathArguments, Result,
    ReturnType, Signature, Type, TypePath,
};

use crate::SpanError;

pub struct Metrics {
    mod_uses: Vec<ItemUse>,
    metrics: Vec<Metric>,
}

impl TryFrom<ItemMod> for Metrics {
    type Error = Error;

    fn try_from(module: ItemMod) -> Result<Self> {
        let (_, items) = module
            .content
            .ok_or(module.ident.error("Expected module to have content"))?;

        let mut metrics = Vec::new();
        let mut mod_uses = Vec::new();

        for item in items {
            match item {
                Item::Fn(func) => metrics.push(Metric::try_from(func)?),
                Item::Use(item_use) => mod_uses.push(item_use),
                _ => return Err(item.error("Expected a use item or metric function")),
            }
        }

        Ok(Metrics { metrics, mod_uses })
    }
}

impl Metrics {
    pub fn make_uses(&self) -> TokenStream2 {
        quip! { #(#{&self.mod_uses})* }
    }

    pub fn make_struct(&self, used: &HashSet<String>) -> TokenStream2 {
        let aliases = self.metrics.iter().map(|metric| {
            quip! { pub type #{metric.make_type_name()} = #{metric.output.make_type()}; }
        });

        let fields = self.used(used).map(|metric| {
            quip! { pub #{metric.name}: #{metric.make_type_name()}, }
        });

        let methods = self.used(used).map(|metric| metric.method.clone());

        quip! {
            #(#aliases)*

            #[derive(Default)]
            pub struct Metrics {
                #(#fields)*
            }

            impl Metrics {
                #(#methods)*
            }
        }
    }

    pub fn make_trait(&self) -> TokenStream2 {
        quip! {
            #[cfg_attr(test, mockall::automock)]
            pub trait MetricsCollector {
                fn collect_unigram(&mut self, unigram: &crate::ngrams::Unigram, count: f64);
                fn collect_bigram(&mut self, bigram: &crate::ngrams::Bigram, count: f64);
                fn collect_trigram(&mut self, trigram: &crate::ngrams::Trigram, count: f64);
            }
        }
    }

    pub fn make_trait_impl(&self, used: &HashSet<String>) -> TokenStream2 {
        let by_kind = |kind: MetricCategory| {
            self.used(used)
                .filter(move |metric| metric.category == kind)
                .map(|metric| metric.make_assign())
        };

        let unigram_collects = by_kind(MetricCategory::Unigram);
        let bigram_collects = by_kind(MetricCategory::Bigram { with_kind: false });
        let bigram_kind_collects = by_kind(MetricCategory::Bigram { with_kind: true });
        let trigram_collects = by_kind(MetricCategory::Trigram { with_kind: false });
        let trigram_kind_collects = by_kind(MetricCategory::Trigram { with_kind: true });

        quip! {
            impl MetricsCollector for Metrics {
                #[allow(unused_variables)]
                fn collect_unigram(&mut self, unigram: &crate::ngrams::Unigram, count: f64) {
                    #(#unigram_collects)*
                }

                #[allow(unused_variables)]
                fn collect_bigram(&mut self, bigram: &crate::ngrams::Bigram, count: f64) {
                    #(#bigram_collects)*
                    bigram.for_each_kind(|kind| {
                        #(#bigram_kind_collects)*
                    });
                }

                #[allow(unused_variables)]
                fn collect_trigram(&mut self, trigram: &crate::ngrams::Trigram, count: f64) {
                    #(#trigram_collects)*
                    trigram.for_each_kind(|kind| {
                        #(#trigram_kind_collects)*
                    });
                }
            }
        }
    }

    fn used(&self, used: &HashSet<String>) -> impl Iterator<Item = &Metric> {
        self.metrics
            .iter()
            .filter(|metric| used.contains(&metric.make_type_name().to_string()))
    }
}

struct Metric {
    method: ItemFn,
    name: Ident,
    category: MetricCategory,
    output: MetricOutput,
}

impl Metric {
    fn make_type_name(&self) -> Ident {
        format_ident!(
            "{}",
            self.name.to_string().to_pascal_case(),
            span = self.name.span()
        )
    }

    fn make_assign(&self) -> TokenStream2 {
        let field = &self.name;
        let helper = self.name.clone();
        let args = match self.category {
            MetricCategory::Unigram => quip! { unigram, count },
            MetricCategory::Bigram { with_kind: false } => quip! { bigram, count },
            MetricCategory::Bigram { with_kind: true } => quip! { bigram, &kind, count },
            MetricCategory::Trigram { with_kind: false } => quip! { trigram, count },
            MetricCategory::Trigram { with_kind: true } => quip! { trigram, &kind, count },
        };

        match &self.output {
            MetricOutput::Scalar => quip! {
                if let Some(value) = Self::#helper(#args) {
                    self.#field += value;
                }
            },
            MetricOutput::Map(_) => quip! {
                if let Some((key, value)) = Self::#helper(#args) {
                    self.#field.add(key, value);
                }
            },
        }
    }
}

impl TryFrom<ItemFn> for Metric {
    type Error = Error;

    fn try_from(func: ItemFn) -> Result<Self> {
        Ok(Self {
            method: func.clone(),
            name: func.sig.ident.clone(),
            category: MetricCategory::try_from(&func.sig)?,
            output: MetricOutput::try_from(&func.sig.output)?,
        })
    }
}

#[derive(PartialEq, Eq)]
enum MetricCategory {
    Unigram,
    Bigram { with_kind: bool },
    Trigram { with_kind: bool },
}

impl TryFrom<&Signature> for MetricCategory {
    type Error = Error;

    fn try_from(signature: &Signature) -> Result<Self> {
        let input_types: Vec<String> = signature
            .inputs
            .iter()
            .filter_map(|arg| {
                let FnArg::Typed(pat_type) = arg else {
                    return None;
                };
                Some(pat_type.ty.to_token_stream().to_string().replace(" ", ""))
            })
            .collect();

        let category = match input_types.as_slice() {
            [a, f] if a == "&Unigram" && f == "f64" => MetricCategory::Unigram,
            [a, f] if a == "&Bigram" && f == "f64" => MetricCategory::Bigram { with_kind: false },
            [a, b, f] if a == "&Bigram" && b == "&BigramKind" && f == "f64" => {
                MetricCategory::Bigram { with_kind: true }
            }
            [a, f] if a == "&Trigram" && f == "f64" => MetricCategory::Trigram { with_kind: false },
            [a, b, f] if a == "&Trigram" && b == "&TrigramKind" && f == "f64" => {
                MetricCategory::Trigram { with_kind: true }
            }
            _ => return Err(signature.error("Invalid signature")),
        };

        Ok(category)
    }
}

#[derive(Debug, PartialEq, Eq)]
enum MetricOutput {
    Scalar,
    Map(Ident),
}

impl MetricOutput {
    fn make_type(&self) -> TokenStream2 {
        match self {
            MetricOutput::Scalar => quip! { f64 },
            MetricOutput::Map(key) => quip! { crate::map::Map<#key> },
        }
    }
}

impl TryFrom<&ReturnType> for MetricOutput {
    type Error = Error;

    fn try_from(return_type: &ReturnType) -> Result<Self> {
        let err = || return_type.error("Expected return type: Option<f64> or Option<(K, f64)>");

        let ReturnType::Type(_, ty) = return_type else {
            return Err(err());
        };

        let Type::Path(TypePath { path, .. }) = ty.as_ref() else {
            return Err(err());
        };

        let segment = path.segments.last().ok_or_else(err)?;
        if segment.ident != "Option" {
            return Err(err());
        }

        let PathArguments::AngleBracketed(ref args) = segment.arguments else {
            return Err(err());
        };

        let Some(GenericArgument::Type(inner)) = args.args.first() else {
            return Err(err());
        };

        match inner {
            Type::Path(p) if p.path.is_ident("f64") => Ok(MetricOutput::Scalar),
            Type::Tuple(tuple) if tuple.elems.len() == 2 => {
                let second = &tuple.elems[1];
                if !matches!(second, Type::Path(p) if p.path.is_ident("f64")) {
                    return Err(second.error("Expected second tuple element to be 'f64'"));
                }

                let first = &tuple.elems[0];
                let Type::Path(path) = first else {
                    return Err(first.error("Expected a simple type identifier as map key"));
                };
                let ident = path
                    .path
                    .get_ident()
                    .ok_or_else(|| first.error("Expected a simple type identifier as map key"))?;

                Ok(MetricOutput::Map(ident.clone()))
            }
            _ => Err(err()),
        }
    }
}
