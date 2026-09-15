//! FreeSWITCH's C that `freeswitch-types` ports, compiled from every tree `hooks/source-refs.yaml`
//! names, for differential tests of the port against each.
//!
//! Every call works on bytes, and the C reads its input up to the first NUL.

use std::ffi::{c_char, c_uint, CStr};

/// One FreeSWITCH tree the build compiles: the pin, or an entry of `trees` in the index.
#[derive(Debug)]
pub struct Tree {
    name: &'static str,
    commit: &'static str,
    public: bool,
    abi: Result<&'static Abi, &'static str>,
}

impl Tree {
    /// `pin` for the commit the source references index, else the name the index gives.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// The commit compiled.
    pub fn commit(&self) -> &'static str {
        self.commit
    }

    /// The index names a public remote for the commit, which CI fetches.
    pub fn is_public(&self) -> bool {
        self.public
    }

    /// The tree's C, or why the build found no source for it.
    pub fn oracle(&self) -> Result<Oracle, &'static str> {
        self.abi
            .map(|abi| Oracle { abi })
    }
}

/// Every tree the index names, the pin first, built or not.
pub fn trees() -> &'static [Tree] {
    TREES
}

/// The C of one built tree.
#[derive(Debug, Clone, Copy)]
pub struct Oracle {
    abi: &'static Abi,
}

type Split = unsafe extern "C" fn(*mut c_char, c_char, *mut *mut c_char, c_uint) -> c_uint;

#[derive(Debug)]
struct Abi {
    cleanup: unsafe extern "C" fn(*mut c_char, c_char) -> *mut c_char,
    char_delim: Split,
    blank_delim: unsafe extern "C" fn(*mut c_char, *mut *mut c_char, c_uint) -> c_uint,
    separate_string: Split,
    separate_string_string:
        unsafe extern "C" fn(*mut c_char, *mut c_char, *mut *mut c_char, c_uint) -> c_uint,
    find_end_paren: unsafe extern "C" fn(*const c_char, c_char, c_char) -> *mut c_char,
}

/// Link one tree's prefixed symbols into a module holding its `ABI`.
#[cfg(c_oracle)]
macro_rules! tree_abi {
    ($tree:ident, $prefix:literal) => {
        mod $tree {
            use std::ffi::{c_char, c_uint};

            extern "C" {
                #[link_name = concat!($prefix, "oracle_cleanup")]
                fn cleanup(str: *mut c_char, delim: c_char) -> *mut c_char;
                #[link_name = concat!($prefix, "oracle_char_delim")]
                fn char_delim(
                    buf: *mut c_char,
                    delim: c_char,
                    array: *mut *mut c_char,
                    arraylen: c_uint,
                ) -> c_uint;
                #[link_name = concat!($prefix, "oracle_blank_delim")]
                fn blank_delim(
                    buf: *mut c_char,
                    array: *mut *mut c_char,
                    arraylen: c_uint,
                ) -> c_uint;
                #[link_name = concat!($prefix, "switch_separate_string")]
                fn separate_string(
                    buf: *mut c_char,
                    delim: c_char,
                    array: *mut *mut c_char,
                    arraylen: c_uint,
                ) -> c_uint;
                #[link_name = concat!($prefix, "switch_separate_string_string")]
                fn separate_string_string(
                    buf: *mut c_char,
                    delim: *mut c_char,
                    array: *mut *mut c_char,
                    arraylen: c_uint,
                ) -> c_uint;
                #[link_name = concat!($prefix, "switch_find_end_paren")]
                fn find_end_paren(s: *const c_char, open: c_char, close: c_char) -> *mut c_char;
            }

            pub(super) static ABI: super::Abi = super::Abi {
                cleanup,
                char_delim,
                blank_delim,
                separate_string,
                separate_string_string,
                find_end_paren,
            };
        }
    };
}

include!(concat!(env!("OUT_DIR"), "/trees.rs"));

/// `input` and its terminator. After a trailing backslash the C steps over the terminator and
/// reads the next byte, so a second NUL keeps that read inside the buffer and ends the split.
fn buffer(input: &[u8]) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(input.len() + 2);
    buffer.extend_from_slice(input);
    buffer.extend_from_slice(&[0, 0]);
    buffer
}

/// The strings `array` points at, each into a buffer still alive.
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

impl Oracle {
    /// `cleanup_separated_string`, `delim` 0 where the switch passes none.
    pub fn cleanup(self, input: &[u8], delim: u8) -> Vec<u8> {
        let mut buffer = buffer(input);
        // SAFETY: the buffer is NUL-terminated; the C rewrites it in place and returns a pointer
        // into it.
        let start = unsafe {
            (self
                .abi
                .cleanup)(
                buffer
                    .as_mut_ptr()
                    .cast(),
                delim as c_char,
            )
        };
        tokens(&[start]).remove(0)
    }

    /// `switch_separate_string`, `^^X` head included, keeping at most `limit` tokens.
    pub fn separate_string(self, input: &[u8], delim: u8, limit: u32) -> Vec<Vec<u8>> {
        split(input, limit, |buf, array, len| {
            // SAFETY: `buf` is NUL-terminated and `array` holds `len` slots.
            unsafe {
                (self
                    .abi
                    .separate_string)(buf, delim as c_char, array, len)
            }
        })
    }

    /// `separate_string_char_delim` with no `^^X` head read.
    pub fn char_delim(self, input: &[u8], delim: u8, limit: u32) -> Vec<Vec<u8>> {
        split(input, limit, |buf, array, len| {
            // SAFETY: `buf` is NUL-terminated and `array` holds `len` slots.
            unsafe {
                (self
                    .abi
                    .char_delim)(buf, delim as c_char, array, len)
            }
        })
    }

    /// `separate_string_blank_delim` with no `^^X` head read.
    pub fn blank_delim(self, input: &[u8], limit: u32) -> Vec<Vec<u8>> {
        split(input, limit, |buf, array, len| {
            // SAFETY: `buf` is NUL-terminated and `array` holds `len` slots.
            unsafe {
                (self
                    .abi
                    .blank_delim)(buf, array, len)
            }
        })
    }

    /// `switch_separate_string_string`, keeping at most `limit` tokens.
    pub fn separate_string_string(self, input: &[u8], delim: &[u8], limit: u32) -> Vec<Vec<u8>> {
        let mut delim = buffer(delim);
        split(input, limit, |buf, array, len| {
            // SAFETY: both strings are NUL-terminated and `array` holds `len` slots.
            unsafe {
                (self
                    .abi
                    .separate_string_string)(
                    buf,
                    delim
                        .as_mut_ptr()
                        .cast(),
                    array,
                    len,
                )
            }
        })
    }

    /// `switch_find_end_paren`: the byte offset of the close, or `None` when there is none.
    pub fn find_end_paren(self, input: &[u8], open: u8, close: u8) -> Option<usize> {
        let buffer = buffer(input);
        // SAFETY: the buffer is NUL-terminated and the C only reads it.
        let end = unsafe {
            (self
                .abi
                .find_end_paren)(
                buffer
                    .as_ptr()
                    .cast(),
                open as c_char,
                close as c_char,
            )
        };
        // SAFETY: a non-null result points into the same buffer.
        (!end.is_null()).then(|| unsafe {
            end.cast_const()
                .offset_from(
                    buffer
                        .as_ptr()
                        .cast(),
                )
        } as usize)
    }
}
