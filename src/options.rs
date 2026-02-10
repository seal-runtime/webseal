use libc::c_int;
use bstr::{BString, ByteSlice};

use crate::ffi;

use crate::utils::*;

pub struct WebviewOptions {
    pub title: String,
    pub html: String,
    pub size: (f32, f32),
    pub resizeable: bool,
    pub max_size: Option<(f32, f32)>,
    pub min_size: Option<(f32, f32)>,
}
impl WebviewOptions {
    /// SAFETY: element at stack idx -1 must be a vector
    unsafe fn x_and_y_from_vector(state: *mut ffi::lua_State) -> (f32, f32) {
        // yeah so the luau engineers had this brilliant idea
        // and make accessing x, y, z be pointer offsets from x. so very safe
        // i guess that's just what an array is in C
        // SAFETY: we know value at stack idx -1 is a vector therefore
        // y and z are guaranteed to be pointer offsets 
        // of the size of f32 away from the pointer returned by lua_tovector (pointer to x)
        // we only need x and y, so we only need to move the pointer by 1 f32 to the right
        let x_ptr = unsafe { ffi::lua_tovector(state, -1) };
        let x = unsafe { *x_ptr };
        let y_ptr = unsafe { x_ptr.add(1) };
        let y = unsafe { *y_ptr };
        (x, y)
    }
    /// Extracts relevant values from the table passed to webview.create;
    /// - If there's an error, pushes the wrapped_error onto the stack
    /// - If there's a passed event handler function, pushes it to the Luau registry as `WEBSEAL_WEBVIEW_HANDLER`
    /// # Safety
    /// - `state` must be a pointer to a non-null Luau state
    /// - The value at stack index -1 must be a Luau table.
    pub unsafe fn from_table_on_stack(state: *mut ffi::lua_State, function_name: &'static str) -> Result<Self, c_int> {
        let title_type = unsafe { ffi::lua_getfield(state, -1, c"title".as_ptr()) };
        let title = if title_type == ffi::LUA_TSTRING {
            let ptr = unsafe { ffi::lua_tostring(state, -1) };
            let s = unsafe { BString::clone_from_ptr(ptr) }.to_str_lossy().to_string();
            s
        } else {
            String::from("seal")
        };
        // get rid of title to balance stack
        unsafe { ffi::lua_pop(state, 1) };

        let html_type = unsafe { ffi::lua_getfield(state, -1, c"html".as_ptr()) };
        let html = if html_type == ffi::LUA_TSTRING {
            let ptr = unsafe { ffi::lua_tostring(state, -1) };
            let s = unsafe { BString::clone_from_ptr(ptr) }.to_str_lossy().to_string();
            // get rid of html to balance stack
            unsafe { ffi::lua_pop(state, 1) };
            s
        } else {
            push_wrapped_error(state, &format!("{}: missing or incorrect table field 'html' (got {})", function_name, unsafe { type_of(state, -1) }));
            return Err(1);
        };

        let size_type = unsafe { ffi::lua_getfield(state, -1, c"size".as_ptr()) };
        let size = if size_type == ffi::LUA_TVECTOR {
            unsafe { Self::x_and_y_from_vector(state) }
        } else {
            (420.0, 600.0)
        };
        // get rid of the vector or nil to balance stack
        unsafe { ffi::lua_pop(state, 1) };

        let resizeable_type = unsafe { ffi::lua_getfield(state, -1, c"resizeable".as_ptr()) };
        let resizeable = if resizeable_type == ffi::LUA_TBOOLEAN {
            let b = unsafe { ffi::lua_toboolean(state, -1) };
            match b {
                0 => false,
                1 => true,
                _ => unreachable!("booleaning not boolington")
            }
        } else {
            true
        };
        unsafe { ffi::lua_pop(state, 1) };

        let min_size_type = unsafe { ffi::lua_getfield(state, -1, c"min_size".as_ptr()) };
        let min_size = if min_size_type == ffi::LUA_TVECTOR {
            Some(unsafe { Self::x_and_y_from_vector(state) })
        } else {
            None
        };
        unsafe { ffi::lua_pop(state, 1) };

        let max_size_type = unsafe { ffi::lua_getfield(state, -1, c"max_size".as_ptr()) };
        let max_size = if max_size_type == ffi::LUA_TVECTOR {
            Some(unsafe { Self::x_and_y_from_vector(state) })
        } else {
            None
        };
        unsafe { ffi::lua_pop(state, 1) };

        Ok(Self {
            title,
            html,
            size,
            resizeable,
            min_size,
            max_size,
        })
    }
}