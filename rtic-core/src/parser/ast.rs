use std::sync::atomic::Ordering;

use heck::ToSnakeCase;
use proc_macro2::Span;
use quote::ToTokens;
use syn::{
    parse::Parser, spanned::Spanned, Expr, ExprArray, Ident, ItemFn, ItemImpl, ItemStruct, LitInt,
    Meta,
};

use crate::DEFAULT_TASK_PRIORITY;

#[derive(Debug)]
pub struct InitTask {
    pub args: InitTaskArgs,
    pub ident: Ident,
    pub body: ItemFn,
}

#[derive(Debug, Clone, Default)]
pub struct InitTaskArgs {
    pub core: u32,
}

impl InitTaskArgs {
    pub fn parse(args: Meta) -> syn::Result<Self> {
        let mut core: Option<syn::LitInt> = None;
        let Meta::List(args) = args else {
            return Ok(Self::default());
        };

        syn::meta::parser(|meta| {
            if meta.path.is_ident("core") {
                core = Some(meta.value()?.parse()?)
            } else {
                // this is needed to advance the values iterator
                let _ = meta.value()?.parse::<Expr>();
            }
            Ok(())
        })
        .parse2(args.tokens)?;

        let core = core
            .and_then(|core| core.base10_parse().ok())
            .unwrap_or_default();

        Ok(Self { core })
    }
}

#[derive(Debug)]
pub struct TaskArgs {
    pub interrupt_handler_name: Option<syn::Ident>,
    pub priority: u16,
    // list of identifiers for shared resources
    pub shared_idents: Vec<Ident>,
    pub core: u32,
}

impl TaskArgs {
    pub fn parse(args: Meta) -> syn::Result<Self> {
        let Meta::List(args) = args else {
            return Ok(TaskArgs {
                interrupt_handler_name: None,
                priority: DEFAULT_TASK_PRIORITY.load(Ordering::Relaxed),
                shared_idents: Default::default(),
                core: 0,
            });
        };

        let mut interrupt_handler_name: Option<syn::Path> = None;
        let mut priority: Option<LitInt> = None;
        let mut shared: Option<ExprArray> = None;
        let mut core: Option<LitInt> = None;

        syn::meta::parser(|meta| {
            if meta.path.is_ident("binds") {
                interrupt_handler_name = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("priority") {
                priority = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("shared") {
                shared = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("core") {
                core = Some(meta.value()?.parse()?);
            } else {
                // this is needed to advance the values iterator
                let _: syn::Result<Expr> = meta.value()?.parse();
            }
            Ok(())
        })
        .parse2(args.tokens)?;

        let interrupt_handler_name = interrupt_handler_name
            .map(|i| Ident::new(&i.to_token_stream().to_string(), Span::call_site()));

        let priority = priority
            .and_then(|p| p.base10_parse().ok())
            .unwrap_or(DEFAULT_TASK_PRIORITY.load(Ordering::Relaxed));

        let core = core
            .and_then(|core| core.base10_parse().ok())
            .unwrap_or_default();

        let shared_idents = if let Some(shared) = shared {
            let mut elements = Vec::with_capacity(shared.elems.len());
            for element in shared.elems {
                let element = Ident::new(&element.to_token_stream().to_string(), Span::call_site());
                elements.push(element);
            }
            elements
        } else {
            Vec::new()
        };

        Ok(Self {
            interrupt_handler_name,
            priority,
            shared_idents,
            core,
        })
    }
}

/// Alias for hardware task
pub type HardwareTask = RticTask;

/// Alias for idle tasks. idle task has `interrupt_handler_name` set to None and priority 0
pub type IdleTask = RticTask;

#[derive(Debug)]
pub struct RticTask {
    pub args: TaskArgs,
    pub task_struct: ItemStruct,
    pub struct_impl: ItemImpl,
}

impl RticTask {
    pub fn name(&self) -> &Ident {
        &self.task_struct.ident
    }

    pub fn name_uppercase(&self) -> Ident {
        let name = self
            .task_struct
            .ident
            .to_string()
            .to_snake_case()
            .to_uppercase();
        Ident::new(&name, Span::call_site())
    }

    pub fn name_snakecase(&self) -> Ident {
        let name = self.task_struct.ident.to_string().to_snake_case();
        Ident::new(&name, Span::call_site())
    }
}

#[derive(Debug, Clone)]
pub struct SharedElement {
    pub ident: Ident,
    pub ty: syn::Type,
    pub priority: u16,
}

#[derive(Debug, Clone, Default)]
pub struct SharedResourcesArgs {
    pub core: u32,
}

impl SharedResourcesArgs {
    pub fn parse(args: Meta) -> syn::Result<Self> {
        let mut core: Option<syn::LitInt> = None;
        let Meta::List(args) = args else {
            return Ok(Self::default());
        };

        syn::meta::parser(|meta| {
            if meta.path.is_ident("core") {
                core = Some(meta.value()?.parse()?)
            } else {
                // this is needed to advance the values iterator
                let _ = meta.value()?.parse::<Expr>();
            }
            Ok(())
        })
        .parse2(args.tokens)?;

        let core = core
            .and_then(|core| core.base10_parse().ok())
            .unwrap_or_default();

        Ok(Self { core })
    }
}

#[derive(Debug, Clone)]
pub struct SharedResources {
    pub args: SharedResourcesArgs,
    pub strct: ItemStruct,
    pub resources: Vec<SharedElement>,
}

impl SharedResources {
    pub fn get_field_mut(&mut self, field_name: &Ident) -> Option<&mut SharedElement> {
        self.resources
            .iter_mut()
            .find(|field| &field.ident == field_name)
    }

    pub fn get_field(&self, field_name: &Ident) -> Option<&SharedElement> {
        self.resources
            .iter()
            .find(|field| &field.ident == field_name)
    }
    pub fn name_uppercase(&self) -> Ident {
        let name = self.strct.ident.to_string().to_snake_case().to_uppercase();
        Ident::new(&name, Span::call_site())
    }
}

#[derive(Debug)]
pub struct AppArgs {
    // path to peripheral crate
    pub device: syn::Path,
    pub peripherals: bool,
    pub cores: u32,
}

impl AppArgs {
    pub fn parse(args: proc_macro2::TokenStream) -> syn::Result<Self> {
        let args_span = args.span();
        let mut device: Option<syn::Path> = None;
        let mut peripherals: Option<syn::LitBool> = None;
        let mut cores: Option<syn::LitInt> = None;
        syn::meta::parser(|meta| {
            if meta.path.is_ident("device") {
                device = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("peripherals") {
                peripherals = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("cores") {
                cores = Some(meta.value()?.parse()?)
            } else {
                // this is needed to advance the values iterator
                let _: syn::Result<syn::Expr> = meta.value()?.parse();
            }
            Ok(())
        })
        .parse2(args)?;

        let Some(device) = device else {
            return Err(syn::Error::new(
                args_span,
                "device = path::to:pac must be provided.",
            ));
        };

        let cores = cores
            .and_then(|cores| cores.base10_parse().ok())
            .unwrap_or(1_u32);

        Ok(Self {
            device,
            peripherals: peripherals.map_or(false, |f| f.value),
            cores,
        })
    }
}
