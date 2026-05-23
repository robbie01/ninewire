use core::slice;
use std::{error::Error, ffi::{CStr, c_void}, fmt::Display, marker::PhantomData, ops::Deref, ptr};

use mpv_sys::*;

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct Node<'a> {
    inner: mpv_node,
    _phantom: PhantomData<&'a ()>
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct NodeSlice<'a> {
    list: mpv_node_list,
    _phantom: PhantomData<&'a [Node<'a>]>
}

impl<'a> From<&'a [Node<'a>]> for NodeSlice<'a> {
    fn from(value: &'a [Node<'a>]) -> Self {
        Self {
            list: mpv_node_list {
                num: value.len().try_into().unwrap(),
                values: value.as_ptr() as *mut mpv_node,
                keys: ptr::null_mut()
            },
            _phantom: PhantomData
        }
    }
}

impl<'a> From<NodeSlice<'a>> for &'a [Node<'a>] {
    fn from(value: NodeSlice<'a>) -> Self {
        if value.list.values.is_null() {
            assert_eq!(value.list.num, 0);
            &[]
        } else {
            unsafe {
                slice::from_raw_parts(
                    value.list.values as *const _,
                    value.list.num.try_into().unwrap()
                )
            }
        }
    }
}

impl<'a> Deref for NodeSlice<'a> {
    type Target = [Node<'a>];

    fn deref(&self) -> &Self::Target {
        (*self).into()
    }
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ByteSlice<'a> {
    array: mpv_byte_array,
    _phantom: PhantomData<&'a [u8]>
}

impl<'a> From<&'a [u8]> for ByteSlice<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self {
            array: mpv_byte_array {
                data: value.as_ptr() as *mut c_void,
                size: value.len()
            },
            _phantom: PhantomData
        }
    }
}

impl<'a> From<ByteSlice<'a>> for &'a [u8] {
    fn from(value: ByteSlice<'a>) -> Self {
        if value.array.data.is_null() {
            assert_eq!(value.array.size, 0);
            &[]
        } else {
            unsafe {
                slice::from_raw_parts(
                    value.array.data as *const u8,
                    value.array.size
                )
            }
        }
    }
}

impl<'a> Deref for ByteSlice<'a> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        (*self).into()
    }
}

#[derive(Clone, Copy)]
pub enum NodeValue<'a> {
    None,
    String(&'a CStr),
    Flag(bool),
    Int64(i64),
    Double(f64),
    NodeArray(&'a NodeSlice<'a>),
    ByteArray(&'a ByteSlice<'a>)
}

impl<'a> From<NodeValue<'a>> for Node<'a> {
    fn from(value: NodeValue<'a>) -> Self {
        match value {
            NodeValue::None => Self {
                inner: mpv_node {
                    u: mpv_node__bindgen_ty_1 { int64: 0 },
                    format: MPV_FORMAT_NONE
                },
                _phantom: PhantomData
            },
            NodeValue::String(v) => Self {
                inner: mpv_node {
                    u: mpv_node__bindgen_ty_1 { string: v.as_ptr() as *mut _ },
                    format: MPV_FORMAT_STRING
                },
                _phantom: PhantomData
            },
            NodeValue::Flag(v) => Self {
                inner: mpv_node {
                    u: mpv_node__bindgen_ty_1 { flag: v as i32 },
                    format: MPV_FORMAT_FLAG
                },
                _phantom: PhantomData
            },
            NodeValue::Int64(v) => Self {
                inner: mpv_node {
                    u: mpv_node__bindgen_ty_1 { int64: v },
                    format: MPV_FORMAT_INT64
                },
                _phantom: PhantomData
            },
            NodeValue::Double(v) => Self {
                inner: mpv_node {
                    u: mpv_node__bindgen_ty_1 { double_: v },
                    format: MPV_FORMAT_DOUBLE
                },
                _phantom: PhantomData
            },
            NodeValue::NodeArray(v) => Self {
                inner: mpv_node {
                    u: mpv_node__bindgen_ty_1 { list: &v.list as *const _ as *mut _ },
                    format: MPV_FORMAT_NODE_ARRAY
                },
                _phantom: PhantomData
            },
            NodeValue::ByteArray(v) => Self {
                inner: mpv_node {
                    u: mpv_node__bindgen_ty_1 { list: &v.array as *const _ as *mut _ },
                    format: MPV_FORMAT_BYTE_ARRAY
                },
                _phantom: PhantomData
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct UnsupportedType(u32);

impl Display for UnsupportedType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported node type {}", self.0)
    }
}

impl Error for UnsupportedType {}

impl<'a> TryFrom<Node<'a>> for NodeValue<'a> {
    type Error = UnsupportedType;

    fn try_from(value: Node<'a>) -> Result<Self, Self::Error> {
        Ok(match value.inner.format {
            MPV_FORMAT_NONE => Self::None,
            MPV_FORMAT_STRING => Self::String(unsafe { CStr::from_ptr(value.inner.u.string) }),
            MPV_FORMAT_FLAG => Self::Flag(unsafe { value.inner.u.flag } != 0),
            MPV_FORMAT_INT64 => Self::Int64(unsafe { value.inner.u.int64 }),
            MPV_FORMAT_DOUBLE => Self::Double(unsafe { value.inner.u.double_ }),
            MPV_FORMAT_NODE_ARRAY => Self::NodeArray(unsafe { &*(value.inner.u.list as *const NodeSlice<'a>) }),
            MPV_FORMAT_BYTE_ARRAY => Self::ByteArray(unsafe { &*(value.inner.u.ba as *const ByteSlice<'a>) }),

            f => return Err(UnsupportedType(f))
        })
    }
}