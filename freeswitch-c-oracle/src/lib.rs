//! FreeSWITCH's C that `freeswitch-types` ports, compiled from every tree `hooks/source-refs.yaml`
//! names, for differential tests of the port against each.
//!
//! Every call works on bytes, and the C reads its input up to the first NUL. Passes that install
//! channel variables or call into the core run against stubs that report each call and store
//! nothing: a variable lookup or API call answers nothing.

use std::ffi::{c_char, c_int, c_long, c_uint, c_void, CStr};

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
type Emit = unsafe extern "C" fn(*mut c_void, c_int, *const c_char, *const c_char);
type Recorded = unsafe extern "C" fn(*const c_char, Emit, *mut c_void);

#[derive(Debug)]
struct Abi {
    cleanup: unsafe extern "C" fn(*mut c_char, c_char) -> *mut c_char,
    char_delim: Split,
    blank_delim: unsafe extern "C" fn(*mut c_char, *mut *mut c_char, c_uint) -> c_uint,
    separate_string: Split,
    separate_string_string:
        unsafe extern "C" fn(*mut c_char, *mut c_char, *mut *mut c_char, c_uint) -> c_uint,
    find_end_paren: unsafe extern "C" fn(*const c_char, c_char, c_char) -> *mut c_char,
    url_encode_opt: unsafe extern "C" fn(*const c_char, *mut c_char, usize, c_int) -> *mut c_char,
    url_encode: unsafe extern "C" fn(*const c_char, *mut c_char, usize) -> *mut c_char,
    needs_url_encode: unsafe extern "C" fn(*const c_char) -> c_int,
    core_url_encode_opt: unsafe extern "C" fn(*const c_char, c_int, *mut c_char, usize) -> usize,
    url_unsafe: unsafe extern "C" fn() -> *const c_char,
    brackets:
        unsafe extern "C" fn(*mut c_char, c_char, c_char, c_char, Emit, *mut c_void) -> c_long,
    dial: Recorded,
    expand: Recorded,
    api_originate: Recorded,
    switch_true: unsafe extern "C" fn(*const c_char) -> c_int,
}

/// Link one tree's prefixed symbols into a module holding its `ABI`.
#[cfg(c_oracle)]
macro_rules! tree_abi {
    ($tree:ident, $prefix:literal) => {
        mod $tree {
            use std::ffi::{c_char, c_int, c_long, c_uint, c_void};

            use super::Emit;

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
                #[link_name = concat!($prefix, "switch_url_encode_opt")]
                fn url_encode_opt(
                    url: *const c_char,
                    buf: *mut c_char,
                    len: usize,
                    double_encode: c_int,
                ) -> *mut c_char;
                #[link_name = concat!($prefix, "switch_url_encode")]
                fn url_encode(url: *const c_char, buf: *mut c_char, len: usize) -> *mut c_char;
                #[link_name = concat!($prefix, "oracle_needs_url_encode")]
                fn needs_url_encode(s: *const c_char) -> c_int;
                #[link_name = concat!($prefix, "oracle_core_url_encode_opt")]
                fn core_url_encode_opt(
                    url: *const c_char,
                    double_encode: c_int,
                    out: *mut c_char,
                    outlen: usize,
                ) -> usize;
                #[link_name = concat!($prefix, "oracle_url_unsafe")]
                fn url_unsafe() -> *const c_char;
                #[link_name = concat!($prefix, "oracle_brackets")]
                fn brackets(
                    data: *mut c_char,
                    a: c_char,
                    b: c_char,
                    c: c_char,
                    emit: Emit,
                    ctx: *mut c_void,
                ) -> c_long;
                #[link_name = concat!($prefix, "oracle_dial")]
                fn dial(bridgeto: *const c_char, emit: Emit, ctx: *mut c_void);
                #[link_name = concat!($prefix, "oracle_expand")]
                fn expand(input: *const c_char, emit: Emit, ctx: *mut c_void);
                #[link_name = concat!($prefix, "oracle_api_originate")]
                fn api_originate(arg: *const c_char, emit: Emit, ctx: *mut c_void);
                #[link_name = concat!($prefix, "oracle_switch_true")]
                fn switch_true(expr: *const c_char) -> c_int;
            }

            pub(super) static ABI: super::Abi = super::Abi {
                cleanup,
                char_delim,
                blank_delim,
                separate_string,
                separate_string_string,
                find_end_paren,
                url_encode_opt,
                url_encode,
                needs_url_encode,
                core_url_encode_opt,
                url_unsafe,
                brackets,
                dial,
                expand,
                api_originate,
                switch_true,
            };
        }
    };
}

include!(concat!(env!("OUT_DIR"), "/trees.rs"));

/// A header an extracted pass installed: its name, then its value.
pub type Pair = (Vec<u8>, Vec<u8>);

/// What `switch_event_create_brackets` read of the block opening its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Brackets {
    /// Every header installed, in order.
    pub pairs: Vec<Pair>,
    /// The offset of the data after the block.
    pub rest: usize,
    /// The string at `rest` as the parse left it.
    pub following: Vec<u8>,
}

/// What `switch_ivr_originate` and `switch_ivr_enterprise_originate` read of a dial string, up to
/// each leg's endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dial {
    /// `<>` headers ahead of an enterprise split.
    pub enterprise: Vec<Pair>,
    /// One per `:_:` thread, or the whole dial string when there is none.
    pub threads: Vec<Thread>,
    /// Why the enterprise originate stopped before its threads.
    pub failure: Option<Vec<u8>>,
}

/// One thread of a [`Dial`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Thread {
    /// `switch_stristr` found `origination_nested_vars=true` in the thread's text.
    pub nested_vars: bool,
    /// `<>` and `{}` headers.
    pub pairs: Vec<Pair>,
    /// The `|` groups, each its `,` legs.
    pub groups: Vec<Vec<Leg>>,
    /// Why the thread's originate stopped.
    pub failure: Option<Vec<u8>>,
}

/// One leg of a [`Thread`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Leg {
    /// `[]` headers.
    pub pairs: Vec<Pair>,
    /// The text after the leg's blocks, `None` where the originate stopped first.
    pub endpoint: Option<Vec<u8>>,
}

/// What `switch_channel_expand_variables_check` made of its input.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Expansion {
    /// The output, every reference substituted with nothing.
    pub text: Vec<u8>,
    /// Every variable name looked up, in order.
    pub lookups: Vec<Vec<u8>>,
    /// Every API function called, with its argument.
    pub api_calls: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}

/// What `originate_function` did with an API argument line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApiOriginate {
    /// Every line written to the API stream.
    pub output: Vec<Vec<u8>>,
    /// The call to `switch_ivr_originate`, absent when the arguments were refused.
    pub originated: Option<Originated>,
    /// The `switch_assert` expression that failed, which aborts the switch.
    pub assertion: Option<Vec<u8>>,
}

/// The arguments `originate_function` handed on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Originated {
    /// The dial string, `None` for `undef`.
    pub aleg: Option<Vec<u8>>,
    /// Seconds.
    pub timeout: u32,
    /// The caller id name, `None` when absent or `undef`.
    pub cid_name: Option<Vec<u8>>,
    /// The caller id number, `None` when absent or `undef`.
    pub cid_num: Option<Vec<u8>>,
    /// What the new channel runs.
    pub action: Option<Action>,
}

/// What the originated channel runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// `&name(arg)`.
    Application {
        /// Up to the first `(`.
        name: Vec<u8>,
        /// Between the first `(` and the first `)`, `None` without a `(`.
        arg: Option<Vec<u8>>,
    },
    /// `switch_ivr_session_transfer`.
    Transfer {
        /// The extension argument.
        extension: Vec<u8>,
        /// The dialplan, `XML` when absent.
        dialplan: Vec<u8>,
        /// The context, `default` when absent.
        context: Vec<u8>,
    },
}

type Record = (c_int, Option<Vec<u8>>, Option<Vec<u8>>);

/// # Safety
///
/// `ctx` is the `Vec<Record>` [`recorded`] passed, and each non-null string is NUL-terminated.
unsafe extern "C" fn record(ctx: *mut c_void, tag: c_int, a: *const c_char, b: *const c_char) {
    let owned = |s: *const c_char| {
        // SAFETY: the caller guarantees a non-null `s` is NUL-terminated.
        (!s.is_null()).then(|| {
            unsafe { CStr::from_ptr(s) }
                .to_bytes()
                .to_vec()
        })
    };
    // SAFETY: the caller guarantees `ctx` is the records vector, borrowed for this call only.
    let records = unsafe { &mut *ctx.cast::<Vec<Record>>() };
    records.push((tag, owned(a), owned(b)));
}

/// What the harness reported while `call` ran it with [`record`].
fn recorded(call: impl FnOnce(Emit, *mut c_void)) -> Vec<Record> {
    let mut records: Vec<Record> = Vec::new();
    call(record, std::ptr::from_mut(&mut records).cast());
    records
}

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

/// Run a C encoder over a NUL-terminated copy of `url` into a zeroed buffer of `len` bytes.
fn encode_into(
    url: &[u8],
    len: usize,
    call: impl FnOnce(*const c_char, *mut c_char) -> *mut c_char,
) -> Vec<u8> {
    assert!(len > 0, "the C writes a terminator into the last byte");
    let input = buffer(url);
    let mut buf = vec![0u8; len];
    let written = call(
        input
            .as_ptr()
            .cast(),
        buf.as_mut_ptr()
            .cast(),
    );
    assert!(!written.is_null(), "the encoder was given a buffer");
    // SAFETY: the C terminated what it wrote inside `buf`.
    unsafe { CStr::from_ptr(written) }
        .to_bytes()
        .to_vec()
}

fn text(field: Option<Vec<u8>>) -> Vec<u8> {
    field.unwrap_or_default()
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

    /// `switch_url_encode_opt` into a buffer of `len` bytes, terminator included.
    pub fn url_encode_opt(self, url: &[u8], len: usize, double_encode: bool) -> Vec<u8> {
        encode_into(url, len, |url, buf| {
            // SAFETY: `url` is NUL-terminated and `buf` holds `len` bytes, at least one.
            unsafe {
                (self
                    .abi
                    .url_encode_opt)(url, buf, len, c_int::from(double_encode))
            }
        })
    }

    /// `switch_url_encode` into a buffer of `len` bytes, terminator included.
    pub fn url_encode(self, url: &[u8], len: usize) -> Vec<u8> {
        encode_into(url, len, |url, buf| {
            // SAFETY: `url` is NUL-terminated and `buf` holds `len` bytes, at least one.
            unsafe {
                (self
                    .abi
                    .url_encode)(url, buf, len)
            }
        })
    }

    /// `switch_needs_url_encode`, the check `protect_dest_uri` in mod_sofia runs first.
    pub fn needs_url_encode(self, s: &[u8]) -> bool {
        let buffer = buffer(s);
        // SAFETY: the buffer is NUL-terminated and the C only reads it.
        let needs = unsafe {
            (self
                .abi
                .needs_url_encode)(
                buffer
                    .as_ptr()
                    .cast(),
            )
        };
        needs != 0
    }

    /// `switch_core_url_encode_opt`, which sizes its own buffer from a memory pool.
    pub fn core_url_encode_opt(self, url: &[u8], double_encode: bool) -> Vec<u8> {
        let input = buffer(url);
        let mut out = vec![0u8; url.len() * 3 + 1];
        // SAFETY: `input` is NUL-terminated and `out` holds the length passed.
        let len = unsafe {
            (self
                .abi
                .core_url_encode_opt)(
                input
                    .as_ptr()
                    .cast(),
                c_int::from(double_encode),
                out.as_mut_ptr()
                    .cast(),
                out.len(),
            )
        };
        assert!(
            len != usize::MAX,
            "an encoding longer than three bytes per input byte"
        );
        out.truncate(len);
        out
    }

    /// `SWITCH_URL_UNSAFE`.
    pub fn url_unsafe(self) -> &'static [u8] {
        // SAFETY: the C returns a string literal.
        unsafe {
            CStr::from_ptr((self
                .abi
                .url_unsafe)())
        }
        .to_bytes()
    }

    /// `switch_event_create_brackets` on the block opening `data` between `open` and `close`,
    /// splitting pairs on `comma` without a `^^` head, or `None` where the call fails.
    pub fn brackets(self, data: &[u8], open: u8, close: u8, comma: u8) -> Option<Brackets> {
        let mut buffer = buffer(data);
        let mut rest = -1;
        let records = recorded(|emit, ctx| {
            // SAFETY: the buffer is NUL-terminated, and `emit` and `ctx` outlive the call.
            rest = unsafe {
                (self
                    .abi
                    .brackets)(
                    buffer
                        .as_mut_ptr()
                        .cast(),
                    open as c_char,
                    close as c_char,
                    comma as c_char,
                    emit,
                    ctx,
                )
            };
        });
        let rest = usize::try_from(rest).ok()?;
        let following = buffer[rest..]
            .split(|&byte| byte == 0)
            .next()
            .unwrap_or_default()
            .to_vec();
        let pairs = records
            .into_iter()
            .map(|(tag, name, value)| {
                assert_eq!(tag, PAIR, "the bracket parse reports only pairs");
                (text(name), text(value))
            })
            .collect();
        Some(Brackets {
            pairs,
            rest,
            following,
        })
    }

    /// The passes of `switch_ivr_originate`, through its enterprise split, that read `bridgeto`
    /// up to each leg's endpoint.
    pub fn dial(self, bridgeto: &[u8]) -> Dial {
        let mut dial = Dial::default();
        for (tag, a, b) in self.run(
            self.abi
                .dial,
            bridgeto,
        ) {
            if tag == THREAD {
                dial.threads
                    .push(Thread::default());
                continue;
            }
            let thread = dial
                .threads
                .last_mut();
            match (tag, thread) {
                (PAIR, None) => dial
                    .enterprise
                    .push((text(a), text(b))),
                (FAILURE, None) => dial.failure = a,
                (PAIR, Some(thread)) => match thread
                    .groups
                    .last_mut()
                    .and_then(|group| group.last_mut())
                {
                    Some(leg) => leg
                        .pairs
                        .push((text(a), text(b))),
                    None => thread
                        .pairs
                        .push((text(a), text(b))),
                },
                (NESTED, Some(thread)) => thread.nested_vars = true,
                (GROUP, Some(thread)) => thread
                    .groups
                    .push(Vec::new()),
                (LEG, Some(thread)) => thread
                    .groups
                    .last_mut()
                    .expect("a leg follows its group")
                    .push(Leg::default()),
                (ENDPOINT, Some(thread)) => {
                    thread
                        .groups
                        .last_mut()
                        .and_then(|group| group.last_mut())
                        .expect("an endpoint follows its leg")
                        .endpoint = a;
                }
                (FAILURE, Some(thread)) => thread.failure = a,
                (tag, _) => panic!("the dial harness reported tag {tag}"),
            }
        }
        dial
    }

    /// `switch_channel_expand_variables_check` on `input`.
    pub fn expand(self, input: &[u8]) -> Expansion {
        let mut expansion = Expansion::default();
        for (tag, a, b) in self.run(
            self.abi
                .expand,
            input,
        ) {
            match tag {
                OUTPUT => expansion.text = text(a),
                LOOKUP => expansion
                    .lookups
                    .push(text(a)),
                API => expansion
                    .api_calls
                    .push((text(a), b)),
                tag => panic!("the expansion harness reported tag {tag}"),
            }
        }
        expansion
    }

    /// `originate_function` on an API argument line, stripped as `switch_api_execute` strips it.
    pub fn api_originate(self, arg: &[u8]) -> ApiOriginate {
        let mut api = ApiOriginate::default();
        for (tag, a, b) in self.run(
            self.abi
                .api_originate,
            arg,
        ) {
            if tag == ORIGINATE {
                api.originated = Some(Originated {
                    aleg: a,
                    timeout: String::from_utf8(text(b))
                        .ok()
                        .and_then(|timeout| {
                            timeout
                                .parse()
                                .ok()
                        })
                        .expect("the harness prints the timeout as a number"),
                    ..Originated::default()
                });
                continue;
            }
            let originated = api
                .originated
                .as_mut();
            match (tag, originated) {
                (OUTPUT, _) => api
                    .output
                    .push(text(a)),
                (FAILURE, _) => api.assertion = a,
                (CALLER_ID, Some(originated)) => {
                    originated.cid_name = a;
                    originated.cid_num = b;
                }
                (APPLICATION, Some(originated)) => {
                    originated.action = Some(Action::Application {
                        name: text(a),
                        arg: b,
                    });
                }
                (TRANSFER, Some(originated)) => {
                    originated.action = Some(Action::Transfer {
                        extension: text(a),
                        dialplan: text(b),
                        context: Vec::new(),
                    });
                }
                (CONTEXT, Some(originated)) => {
                    if let Some(Action::Transfer { context, .. }) = &mut originated.action {
                        *context = text(a);
                    }
                }
                (tag, _) => panic!("the originate harness reported tag {tag}"),
            }
        }
        api
    }

    /// `switch_true`.
    pub fn switch_true(self, expr: &[u8]) -> bool {
        let buffer = buffer(expr);
        // SAFETY: the buffer is NUL-terminated and the C only reads it.
        let truth = unsafe {
            (self
                .abi
                .switch_true)(
                buffer
                    .as_ptr()
                    .cast(),
            )
        };
        truth != 0
    }

    /// Run a recording harness function over a NUL-terminated copy of `input`.
    fn run(self, call: Recorded, input: &[u8]) -> Vec<Record> {
        let input = buffer(input);
        recorded(|emit, ctx| {
            // SAFETY: the input is NUL-terminated, and `emit` and `ctx` outlive the call.
            unsafe {
                call(
                    input
                        .as_ptr()
                        .cast(),
                    emit,
                    ctx,
                );
            }
        })
    }
}
