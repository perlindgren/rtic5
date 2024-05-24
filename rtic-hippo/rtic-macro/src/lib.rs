use proc_macro::TokenStream;
use proc_macro2::{Ident, TokenStream as TokenStream2};
use quote::{format_ident, quote, ToTokens};

use rtic_core::{AppArgs, RticMacroBuilder, StandardPassImpl, SubAnalysis, SubApp};
use syn::{parse_quote, ItemFn};

extern crate proc_macro;

struct HippoRtic;

use rtic_sw_pass::{SoftwarePass, SoftwarePassImpl};

const MIN_TASK_PRIORITY: u16 = 0; // lowest hippo prio
                                  // const MAX_TASK_PRIORITY: u16 = 3;

#[proc_macro_attribute]
pub fn app(args: TokenStream, input: TokenStream) -> TokenStream {
    // use the standard software pass provided by rtic-sw-pass crate
    let sw_pass = SoftwarePass::new(SwPassBackend);

    let mut builder = RticMacroBuilder::new(HippoRtic);
    builder.bind_pre_std_pass(sw_pass); // run software pass second
    builder.build_rtic_macro(args, input)
}

// =========================================== Trait implementations ===================================================
impl StandardPassImpl for HippoRtic {
    fn default_task_priority(&self) -> u16 {
        MIN_TASK_PRIORITY
    }

    fn post_init(
        &self,
        app_args: &AppArgs,
        _sub_app: &SubApp,
        app_analysis: &SubAnalysis,
    ) -> Option<TokenStream2> {
        let peripheral_crate = &app_args.device;
        let initialize_dispatcher_interrupts =
            app_analysis.used_irqs.iter().map(|(irq_name, priority)| {
                let priority = priority.max(&MIN_TASK_PRIORITY); // limit piority to minmum
                quote! {
                    //set interrupt priority
                    rtic::export::enable(
                        #peripheral_crate::#irq_name,
                        #priority as u8,
                    );
                }
            });

        Some(quote! {
            unsafe {
                #(#initialize_dispatcher_interrupts)*
            }
        })
    }

    fn wfi(&self) -> Option<TokenStream2> {
        None
    }

    fn impl_interrupt_free_fn(&self, mut empty_body_fn: ItemFn) -> ItemFn {
        // eprintln!("{}", empty_body_fn.to_token_stream().to_string()); // enable comment to see the function signature
        let fn_body = parse_quote! {
            {
                rtic::export::interrupt_disable();
                let r = f();
                unsafe { rtic::export::interrupt_enable(); } // critical section end
                r
            }
        };
        empty_body_fn.block = Box::new(fn_body);
        empty_body_fn
    }

    fn compute_lock_static_args(
        &self,
        _app_args: &AppArgs,
        _app_info: &SubApp,
        _app_analysis: &SubAnalysis,
    ) -> Option<TokenStream2> {
        None
    }

    fn impl_resource_proxy_lock(
        &self,
        _app_args: &AppArgs,
        _app_info: &SubApp,
        incomplete_lock_fn: syn::ImplItemFn,
    ) -> syn::ImplItemFn {
        let lock_impl: syn::Block = parse_quote! {
            {
                unsafe { rtic::export::lock(resource_ptr, task_priority as u8, CEILING as u8, f); }
            }
        };

        let mut completed_lock_fn = incomplete_lock_fn;
        completed_lock_fn.block.stmts.extend(lock_impl.stmts);
        completed_lock_fn
    }

    fn entry_name(&self, _core: u32) -> Ident {
        // same entry name for both cores.
        // two main() functions will be generated but both will be guarded by #[cfg(core = "X")]
        // each generated binary will have have one entry
        format_ident!("main")
    }

    /// Customize how the task is dispatched when its bound interrupt is triggered (save baspri before and restore after executing the task)
    fn custom_task_dispatch(
        &self,
        _task_prio: u16,
        _dispatch_task_call: TokenStream2,
    ) -> Option<TokenStream2> {
        None
    }
}

struct SwPassBackend;
impl SoftwarePassImpl for SwPassBackend {
    /// Provide the implementation/body of the core local interrupt pending function.
    fn impl_pend_fn(&self, mut empty_body_fn: ItemFn) -> ItemFn {
        let body = parse_quote!({
            rtic::export::pend(irq_nbr);
        });
        empty_body_fn.block = Box::new(body);
        empty_body_fn
    }

    /// Provide the implementation/body of the cross-core interrupt pending function.
    fn impl_cross_pend_fn(&self, mut empty_body_fn: ItemFn) -> Option<ItemFn> {
        None
    }
}
