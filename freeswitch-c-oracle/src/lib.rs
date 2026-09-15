//! FreeSWITCH's string tokenizer from `switch_utils.c`, compiled at the commit pinned in
//! `hooks/source-refs.yaml`, for differential tests of the port in `freeswitch-types`.
//!
//! Every call works on bytes and answers `None` when the build found no source to compile;
//! [`missing`] says why. The C reads its input up to the first NUL.

#[cfg(c_oracle)]
use std::ffi::{c_char, c_uint, CStr};

#[cfg(c_oracle)]
extern "C" {
    fn oracle_cleanup(str: *mut c_char, delim: c_char) -> *mut c_char;
    fn oracle_char_delim(
        buf: *mut c_char,
        delim: c_char,
        array: *mut *mut c_char,
        arraylen: c_uint,
    ) -> c_uint;
    fn oracle_blank_delim(buf: *mut c_char, array: *mut *mut c_char, arraylen: c_uint) -> c_uint;
    fn switch_separate_string(
        buf: *mut c_char,
        delim: c_char,
        array: *mut *mut c_char,
        arraylen: c_uint,
    ) -> c_uint;
    fn switch_separate_string_string(
        buf: *mut c_char,
        delim: *mut c_char,
        array: *mut *mut c_char,
        arraylen: c_uint,
    ) -> c_uint;
    fn switch_find_end_paren(s: *const c_char, open: c_char, close: c_char) -> *mut c_char;
}

/// Why the oracle was not built, or `None` when it was.
pub fn missing() -> Option<&'static str> {
    if cfg!(c_oracle) {
        None
    } else {
        Some(option_env!("C_ORACLE_MISSING").unwrap_or("the build script named no reason"))
    }
}

/// `input` and its terminator. After a trailing backslash the C steps over the terminator and
/// reads the next byte, so a second NUL keeps that read inside the buffer and ends the split.
#[cfg(c_oracle)]
fn buffer(input: &[u8]) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(input.len() + 2);
    buffer.extend_from_slice(input);
    buffer.extend_from_slice(&[0, 0]);
    buffer
}

/// The strings `array` points at, each into a buffer still alive.
#[cfg(c_oracle)]
fn tokens(array: &[*mut c_char]) -> Vec<Vec<u8>> {
    array
        .iter()
        // SAFETY: the C set every counted slot to a NUL-terminated string inside the buffer.
        .map(|&token| {
            unsafe { CStr::from_ptr(token) }
                .to_bytes()
                .to_vec()
        })
        .collect()
}

/// Run a C split over a NUL-terminated copy of `input` with `limit` slots.
#[cfg(c_oracle)]
fn split(
    input: &[u8],
    limit: u32,
    call: impl FnOnce(*mut c_char, *mut *mut c_char, c_uint) -> c_uint,
) -> Vec<Vec<u8>> {
    let mut buffer = buffer(input);
    let mut array = vec![std::ptr::null_mut::<c_char>(); limit as usize];
    let count = call(
        buffer
            .as_mut_ptr()
            .cast(),
        array.as_mut_ptr(),
        limit,
    );
    tokens(&array[..count as usize])
}

/// `cleanup_separated_string`, `delim` 0 where the switch passes none.
#[cfg(c_oracle)]
pub fn cleanup(input: &[u8], delim: u8) -> Option<Vec<u8>> {
    let mut buffer = buffer(input);
    // SAFETY: the buffer is NUL-terminated; the C rewrites it in place and returns a pointer into it.
    let start = unsafe {
        oracle_cleanup(
            buffer
                .as_mut_ptr()
                .cast(),
            delim as c_char,
        )
    };
    Some(tokens(&[start]).remove(0))
}

/// `switch_separate_string`, `^^X` head included, keeping at most `limit` tokens.
#[cfg(c_oracle)]
pub fn separate_string(input: &[u8], delim: u8, limit: u32) -> Option<Vec<Vec<u8>>> {
    Some(split(input, limit, |buf, array, len| {
        // SAFETY: `buf` is NUL-terminated and `array` holds `len` slots.
        unsafe { switch_separate_string(buf, delim as c_char, array, len) }
    }))
}

/// `separate_string_char_delim` with no `^^X` head read.
#[cfg(c_oracle)]
pub fn char_delim(input: &[u8], delim: u8, limit: u32) -> Option<Vec<Vec<u8>>> {
    Some(split(input, limit, |buf, array, len| {
        // SAFETY: `buf` is NUL-terminated and `array` holds `len` slots.
        unsafe { oracle_char_delim(buf, delim as c_char, array, len) }
    }))
}

/// `separate_string_blank_delim` with no `^^X` head read.
#[cfg(c_oracle)]
pub fn blank_delim(input: &[u8], limit: u32) -> Option<Vec<Vec<u8>>> {
    Some(split(input, limit, |buf, array, len| {
        // SAFETY: `buf` is NUL-terminated and `array` holds `len` slots.
        unsafe { oracle_blank_delim(buf, array, len) }
    }))
}

/// `switch_separate_string_string`, keeping at most `limit` tokens.
#[cfg(c_oracle)]
pub fn separate_string_string(input: &[u8], delim: &[u8], limit: u32) -> Option<Vec<Vec<u8>>> {
    let mut delim = buffer(delim);
    Some(split(input, limit, |buf, array, len| {
        // SAFETY: both strings are NUL-terminated and `array` holds `len` slots.
        unsafe {
            switch_separate_string_string(
                buf,
                delim
                    .as_mut_ptr()
                    .cast(),
                array,
                len,
            )
        }
    }))
}

/// `switch_find_end_paren`: the byte offset of the close, or `Some(None)` when there is none.
#[cfg(c_oracle)]
pub fn find_end_paren(input: &[u8], open: u8, close: u8) -> Option<Option<usize>> {
    let buffer = buffer(input);
    // SAFETY: the buffer is NUL-terminated and the C only reads it.
    let end = unsafe {
        switch_find_end_paren(
            buffer
                .as_ptr()
                .cast(),
            open as c_char,
            close as c_char,
        )
    };
    // SAFETY: a non-null result points into the same buffer.
    Some((!end.is_null()).then(|| unsafe {
        end.cast_const()
            .offset_from(
                buffer
                    .as_ptr()
                    .cast(),
            )
    } as usize))
}

macro_rules! absent {
    ($($(#[$doc:meta])* fn $name:ident($($arg:ident: $ty:ty),*) -> $ret:ty;)*) => {
        $(
            $(#[$doc])*
            #[cfg(not(c_oracle))]
            pub fn $name($(_: $ty),*) -> Option<$ret> {
                None
            }
        )*
    };
}

absent! {
    /// `cleanup_separated_string`, `delim` 0 where the switch passes none.
    fn cleanup(input: &[u8], delim: u8) -> Vec<u8>;
    /// `switch_separate_string`, `^^X` head included, keeping at most `limit` tokens.
    fn separate_string(input: &[u8], delim: u8, limit: u32) -> Vec<Vec<u8>>;
    /// `separate_string_char_delim` with no `^^X` head read.
    fn char_delim(input: &[u8], delim: u8, limit: u32) -> Vec<Vec<u8>>;
    /// `separate_string_blank_delim` with no `^^X` head read.
    fn blank_delim(input: &[u8], limit: u32) -> Vec<Vec<u8>>;
    /// `switch_separate_string_string`, keeping at most `limit` tokens.
    fn separate_string_string(input: &[u8], delim: &[u8], limit: u32) -> Vec<Vec<u8>>;
    /// `switch_find_end_paren`: the byte offset of the close, or `Some(None)` when there is none.
    fn find_end_paren(input: &[u8], open: u8, close: u8) -> Option<usize>;
}
