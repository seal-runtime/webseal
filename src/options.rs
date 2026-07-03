use std::ffi::{CStr, c_int};

use sealbindings::{LuauState, StateExt, SealValue};

pub struct WebviewOptions {
    pub title: String,
    pub html: String,
    pub show_titlebar: bool,
    pub size: (f32, f32),
    pub resizeable: bool,
    pub max_size: Option<(f32, f32)>,
    pub min_size: Option<(f32, f32)>,
    pub always_on_top: bool,
}
impl WebviewOptions {
    /// Creates a WebviewOptions table from table at stack idx -1
    /// 
    /// # Safety
    /// - table must exist at stack idx -1
    /// - luau state must not be null ptr
    pub unsafe fn from_table_on_stack(state: *mut LuauState, function_name: &'static str) -> Result<Self, c_int> {
        // we assume stack idx -1 is already a table
        // stack: [ opts table ]
        
        let _sg = state.stack_returns_none_or_errs();

        let title = Self::get_string_non_nul(state, c"title", function_name)?
            .unwrap_or(String::from("seal"));

        let Some(html) = Self::get_string_non_nul(state, c"html", function_name)? else {
            return Err(state.push_wrapped_error(format!("{} missing required option options.html (should be a string)", function_name)));
        };

        let size = Self::get_opt_vector(state, c"size", function_name)?
            .unwrap_or((420.0, 600.0));

        let resizeable = Self::get_bool(state, c"resizeable", true, function_name)?;
        let show_titlebar = Self::get_bool(state, c"show_titlebar", true, function_name)?;
        let always_on_top = Self::get_bool(state, c"always_on_top", false, function_name)?;

        let min_size = Self::get_opt_vector(state, c"min_size", function_name)?;
        let max_size = Self::get_opt_vector(state, c"max_size", function_name)?;

        Ok(Self {
            title,
            html,
            show_titlebar,
            size,
            resizeable,
            max_size,
            min_size,
            always_on_top
        })
    }

    // helper functions for from_table_on_stack

    /// # Safety
    /// - state must be valid
    /// - stack idx -1 must be luau table
    unsafe fn get_and_pop(state: *mut LuauState, field: &CStr) -> SealValue {
        // put field on stack so we can turn it into an owned SealValue
        unsafe { state.get_field(-1, field) };
        // make owned SealValue for returning
        let value = state.to_seal(-1);
        // get rid of whatever we just put to stack to keep stack balanced
        unsafe { state.pop(1) };

        value
    }

    fn get_string_non_nul(state: *mut LuauState, field: &CStr, function_name: &'static str) -> Result<Option<String>, c_int> {
        let value = unsafe { Self::get_and_pop(state, field) };

        let content = match value {
            SealValue::String(s) => s.to_string(),
            SealValue::Nil => {
                return Ok(None);
            },
            other => {
                return Err(state.push_wrapped_error(format!("{}: options.{} should be a string or nil, got: {:?}", function_name, field.to_string_lossy(), other)));
            }
        };

        if content.contains('\0') {
            return Err(state.push_wrapped_error(format!("{}: options.{} should not contain embedded NUL bytes", function_name, field.to_string_lossy())));
        }

        Ok(Some(content))
    }

    fn get_opt_vector(state: *mut LuauState, field: &CStr, function_name: &'static str) -> Result<Option<(f32, f32)>, c_int> {
        let value = unsafe { Self::get_and_pop(state, field) };

        let tup = match value {
            SealValue::Vector(x, y, _) => {
                (x, y)
            },
            SealValue::Nil => {
                return Ok(None);
            },
            other => {
                return Err(state.push_wrapped_error(format!("{}: expected options.{} to be a vector or nil, got: {:?}", function_name, field.to_string_lossy(), other)));
            }
        };

        Ok(Some(tup))
    }

    fn get_bool(state: *mut LuauState, field: &CStr, default: bool, function_name: &'static str) -> Result<bool, c_int> {
        let value = unsafe { Self::get_and_pop(state, field) };

        match value {
            SealValue::Boolean(b) => Ok(b),
            SealValue::Nil => Ok(default),
            other => {
                return Err(state.push_wrapped_error(format!("{}: expected options.{} to be a boolean or nil, got: {:?}", function_name, field.to_string_lossy(), other)));
            }
        }
    }
}
