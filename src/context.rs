use secp256k1::ContextFlag;
use secp256k1::ffi;
use secp256k1::ffi::Context;
use std::os::raw::c_char;
use std::os::raw::c_void;
use std::convert::From;
use std::default::Default;
use std::mem;
use super::shader;

#[repr(C)]
pub struct secp256k1_fe {
    pub n: [u32; 10],
// #ifdef VERIFY
//     int magnitude;
//     int normalized;
// #endif
}

impl<'a> From<&'a secp256k1_fe> for shader::ty::secp256k1_fe {
    fn from(from: &'a secp256k1_fe) -> Self {
        shader::ty::secp256k1_fe {
            n: from.n,
        }
    }
}

#[repr(C)]
pub struct secp256k1_fe_storage {
    pub n: [u32; 8],
}

impl<'a> From<&'a secp256k1_fe_storage> for shader::ty::secp256k1_fe_storage {
    fn from(from: &'a secp256k1_fe_storage) -> Self {
        shader::ty::secp256k1_fe_storage {
            n: from.n,
        }
    }
}

#[repr(C)]
pub struct secp256k1_ge {
    pub x: secp256k1_fe,
    pub y: secp256k1_fe,
    pub infinity: i32,
}

#[repr(C)]
pub struct secp256k1_gej {
    pub x: secp256k1_fe,
    pub y: secp256k1_fe,
    pub z: secp256k1_fe,
    pub infinity: i32,
}

impl<'a> From<&'a secp256k1_gej> for shader::ty::secp256k1_gej {
    fn from(from: &'a secp256k1_gej) -> Self {
        shader::ty::secp256k1_gej {
            x: (&from.x).into(),
            _dummy0: unsafe { mem::zeroed() },
            y: (&from.y).into(),
            _dummy1: unsafe { mem::zeroed() },
            z: (&from.z).into(),
            _dummy2: unsafe { mem::zeroed() },
            infinity: from.infinity,
        }
    }
}

#[repr(C)]
pub struct secp256k1_ge_storage {
    pub x: secp256k1_fe_storage,
    pub y: secp256k1_fe_storage,
}

impl<'a> From<&'a secp256k1_ge_storage> for shader::ty::secp256k1_ge_storage {
    fn from(from: &'a secp256k1_ge_storage) -> Self {
        shader::ty::secp256k1_ge_storage {
            x: (&from.x).into(),
            _dummy0: unsafe { mem::zeroed() },
            y: (&from.y).into(),
            _dummy1: unsafe { mem::zeroed() },
        }
    }
}

#[repr(C)]
pub struct secp256k1_ecmult_context {
    // secp256k1_ge_storage (*pre_g)[];
    pub pre_g: *mut [secp256k1_ge_storage],
// #ifdef USE_ENDOMORPHISM
//     secp256k1_ge_storage (*pre_g_128)[]; /* odd multiples of 2^128*generator */
// #endif
}

#[repr(C)]
pub struct secp256k1_scalar {
    pub d: [u32; 8],
}

impl<'a> From<&'a secp256k1_scalar> for shader::ty::secp256k1_scalar {
    fn from(from: &'a secp256k1_scalar) -> Self {
        shader::ty::secp256k1_scalar {
            d: from.d,
        }
    }
}

#[repr(C)]
pub struct secp256k1_ecmult_gen_context {
    pub prec: *mut [[secp256k1_ge_storage; 16]; 64],
    pub blind: secp256k1_scalar,
    pub initial: secp256k1_gej,
}

#[repr(C)]
pub struct secp256k1_callback {
    // void (*fn)(const char *text, void* data);
    pub function: *mut fn(*const c_char, *mut c_void),
    // const void* data;
    pub data: *const c_void,
}

#[repr(C)]
pub struct secp256k1_context_struct {
    pub ecmult_ctx: secp256k1_ecmult_context,
    pub ecmult_gen_ctx: secp256k1_ecmult_gen_context,
    pub illegal_callback: secp256k1_callback,
    pub error_callback: secp256k1_callback,
}

pub type secp256k1_context = secp256k1_context_struct;

pub struct Secp256k1Context {
    pub ctx: *mut Context,
    pub caps: ContextFlag,
}

impl Secp256k1Context {
    pub fn new() -> Secp256k1Context {
        Secp256k1Context::with_caps(ContextFlag::Full)
    }

    /// Creates a new Secp256k1 context with the specified capabilities
    pub fn with_caps(caps: ContextFlag) -> Secp256k1Context {
        let flag = match caps {
            ContextFlag::None => ffi::SECP256K1_START_NONE,
            ContextFlag::SignOnly => ffi::SECP256K1_START_SIGN,
            ContextFlag::VerifyOnly => ffi::SECP256K1_START_VERIFY,
            ContextFlag::Full => ffi::SECP256K1_START_SIGN | ffi::SECP256K1_START_VERIFY
        };
        Secp256k1Context { ctx: unsafe { ffi::secp256k1_context_create(flag) }, caps: caps }
    }

    unsafe fn transmute_ctx(&self) -> &secp256k1_context {
        return &*(self.ctx as *mut secp256k1_context)
    }
}

impl<'a> From<&'a Secp256k1Context> for shader::ty::secp256k1_context_struct {
    fn from(from: &'a Secp256k1Context) -> Self {
        unsafe {
            let context = from.transmute_ctx();

            shader::ty::secp256k1_context_struct {
                ctx: shader::ty::secp256k1_ecmult_gen_context {
                    prec: {
                        let mut result = [[mem::uninitialized::<shader::ty::secp256k1_ge_storage>(); 16]; 64];

                        for (subarray_index, subarray) in result.iter_mut().enumerate() {
                            for (index, item) in subarray.iter_mut().enumerate() {
                                *item = (&(*context.ecmult_gen_ctx.prec)[subarray_index][index]).into();
                            }
                        }

                        result
                    },
                    blind: (&context.ecmult_gen_ctx.blind).into(),
                    initial: (&context.ecmult_gen_ctx.initial).into(),
                    _dummy0: unsafe { mem::zeroed() },
                }
            }
        }
    }
}

impl Drop for Secp256k1Context {
    fn drop(&mut self) {
        unsafe { ffi::secp256k1_context_destroy(self.ctx); }
    }
}
