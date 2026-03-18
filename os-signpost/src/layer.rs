//! A [`tracing_subscriber::Layer`] that emits signpost intervals and events.
//!
//! Enable the `tracing` feature to use this module.
//!
//! # Example
//!
//! ```rust,ignore
//! use os_signpost::layer::SignpostLayer;
//! use tracing_subscriber::prelude::*;
//!
//! tracing_subscriber::registry()
//!     .with(SignpostLayer::new("com.example.myapp"))
//!     .init();
//!
//! // Now all tracing spans become signpost intervals in Instruments:
//! #[tracing::instrument]
//! fn do_work() {
//!     tracing::info!("checkpoint");
//! }
//! ```

use std::sync::LazyLock;

use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::registry::LookupSpan;

use crate::{Category, SignpostInterval, Signposter};

/// A [`Layer`] that converts [`tracing`] spans into signpost intervals and
/// [`tracing`] events into signpost point-events.
///
/// Span lifecycle mapping:
/// - `on_new_span` → records the span name
/// - `on_enter` → begins a signpost interval
/// - `on_exit` → ends the signpost interval
/// - `on_event` → emits a signpost point-event
///
/// Spans that are entered multiple times produce one signpost interval per
/// enter/exit pair.
pub struct SignpostLayer {
    subsystem: &'static str,
    category: Category,
}

impl SignpostLayer {
    /// Create a new layer with the given subsystem and `DynamicTracing` category.
    ///
    /// `subsystem` should be a reverse-DNS identifier (e.g. `"com.example.myapp"`).
    /// It must be `&'static str` because the layer lives for the lifetime of the
    /// subscriber.
    pub fn new(subsystem: &'static str) -> Self {
        Self {
            subsystem,
            category: Category::DynamicTracing,
        }
    }

    /// Override the signpost category.
    pub fn with_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    fn signposter(&self) -> &'static Signposter {
        // We need a 'static Signposter. Since Layer is typically created once
        // at startup, we leak the allocation. This is intentional — the
        // signposter must live as long as the process.
        //
        // We use a thread-local cache keyed on subsystem pointer to avoid
        // re-creating when multiple spans hit the same layer. In practice
        // there's exactly one SignpostLayer per subscriber.
        static SIGNPOSTER: LazyLock<std::sync::Mutex<Vec<(&'static str, &'static Signposter)>>> =
            LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

        let mut cache = SIGNPOSTER.lock().unwrap();
        for &(sub, sp) in cache.iter() {
            if sub == self.subsystem {
                return sp;
            }
        }
        let sp: &'static Signposter = Box::leak(Box::new(Signposter::new(
            self.subsystem,
            self.category.clone(),
        )));
        cache.push((self.subsystem, sp));
        sp
    }
}

/// Per-span data stored in the registry extensions.
struct SignpostSpanData {
    /// The signpost interval for the current enter/exit pair.
    /// `Some` while the span is entered, `None` when exited.
    interval: Option<SignpostInterval>,
}

impl<S> Layer<S> for SignpostLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, _attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            span.extensions_mut()
                .insert(SignpostSpanData { interval: None });
        }
    }

    fn on_record(&self, _id: &Id, _values: &Record<'_>, _ctx: Context<'_, S>) {
        // We don't update signpost metadata on record — the interval name
        // is fixed at span creation.
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let name = span.name();
            let interval = self.signposter().begin_interval(name);
            if let Some(data) = span.extensions_mut().get_mut::<SignpostSpanData>() {
                data.interval = Some(interval);
            }
        }
    }

    fn on_exit(&self, id: &Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else { return };
        if let Some(data) = span.extensions_mut().get_mut::<SignpostSpanData>() {
            data.interval.take();
        }
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let meta = event.metadata();
        self.signposter().event(meta.name());
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(&id) else { return };
        if let Some(data) = span.extensions_mut().get_mut::<SignpostSpanData>() {
            data.interval.take();
        }
    }
}
