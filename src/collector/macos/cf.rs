//! Thin CoreFoundation helpers. They take raw CF refs obtained from CF/IOKit calls in this
//! crate and null/type-check them before use, so they are not marked `unsafe`.
#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core_foundation::base::{CFGetTypeID, CFRelease, CFTypeRef, TCFType};
use core_foundation::data::{CFData, CFDataGetTypeID, CFDataRef};
use core_foundation::dictionary::{
    CFDictionaryGetTypeID, CFDictionaryGetValueIfPresent, CFDictionaryRef,
};
use core_foundation::number::{CFNumber, CFNumberGetTypeID, CFNumberRef};
use core_foundation::string::{CFString, CFStringGetTypeID, CFStringRef};

pub fn cfstr(s: &str) -> CFString {
    CFString::new(s)
}

/// Borrowed lookup (Get rule): the returned ref is owned by `dict`.
pub fn dict_get(dict: CFDictionaryRef, key: &str) -> Option<CFTypeRef> {
    if dict.is_null() {
        return None;
    }
    let k = cfstr(key);
    let mut out: *const std::ffi::c_void = std::ptr::null();
    let found =
        unsafe { CFDictionaryGetValueIfPresent(dict, k.as_concrete_TypeRef() as _, &mut out) };
    (found != 0 && !out.is_null()).then_some(out as CFTypeRef)
}

pub fn to_string(s: CFStringRef) -> Option<String> {
    if s.is_null() {
        return None;
    }
    Some(unsafe { CFString::wrap_under_get_rule(s) }.to_string())
}

pub fn to_i64(v: CFTypeRef) -> Option<i64> {
    if v.is_null() || unsafe { CFGetTypeID(v) } != unsafe { CFNumberGetTypeID() } {
        return None;
    }
    unsafe { CFNumber::wrap_under_get_rule(v as CFNumberRef) }.to_i64()
}

pub fn to_data(v: CFTypeRef) -> Option<Vec<u8>> {
    if v.is_null() || unsafe { CFGetTypeID(v) } != unsafe { CFDataGetTypeID() } {
        return None;
    }
    Some(
        unsafe { CFData::wrap_under_get_rule(v as CFDataRef) }
            .bytes()
            .to_vec(),
    )
}

pub fn to_dict(v: CFTypeRef) -> Option<CFDictionaryRef> {
    (!v.is_null() && unsafe { CFGetTypeID(v) } == unsafe { CFDictionaryGetTypeID() })
        .then_some(v as CFDictionaryRef)
}

pub fn is_string(v: CFTypeRef) -> bool {
    !v.is_null() && unsafe { CFGetTypeID(v) } == unsafe { CFStringGetTypeID() }
}

/// Owned CF object released on drop (Create/Copy rule).
pub struct Owned(pub CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CFRelease(self.0) }
        }
    }
}
