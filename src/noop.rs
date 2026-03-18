//! No-op backend for non-Apple platforms and `disable-signposts` feature.

use std::fmt;

use crate::Category;

pub(crate) struct SignposterInner;

impl SignposterInner {
    #[inline(always)]
    pub(crate) fn new(_subsystem: &str, _category: Category) -> Self {
        Self
    }

    #[inline(always)]
    pub(crate) fn enabled(&self) -> bool {
        false
    }

    #[inline(always)]
    pub(crate) fn event(&self, _name: &str, _msg: Option<impl fmt::Display>) {}

    #[inline(always)]
    pub(crate) fn begin_interval(
        &self,
        _name: &str,
        _msg: Option<impl fmt::Display>,
    ) -> SignpostIntervalInner {
        SignpostIntervalInner
    }
}

pub(crate) struct SignpostIntervalInner;

impl SignpostIntervalInner {
    #[inline(always)]
    pub(crate) fn end_with_message(self, _msg: impl fmt::Display) {}
}
