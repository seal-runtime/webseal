use std::ffi::{CString, c_int};

use bstr::{BString, ByteSlice};
use crossbeam_channel::TryRecvError;
use seal::{ffi, push_wrapped_error};

use crate::{ToLuau, ToWindow};

use crate::utils::{self, BStringFromPtr};

pub const WEBVIEW_IPC_TAG: c_int = 13;

pub struct WebviewIpc {
    pub sender: crossbeam_channel::Sender<ToWindow>,
    pub receiver: crossbeam_channel::Receiver<ToLuau>,
}
impl WebviewIpc {
    /// Gets the &WebviewIpc from `idx` on the Luau stack, popping it.
    /// 
    /// Pushes a wrapped error message onto the Luau stack if unable to get the WebviewIpc for whatever reason.
    /// 
    /// Removes the WebviewIpc userdata from the Luau stack if successful
    /// 
    /// # Safety
    /// - make sure `idx` is the CORRECT idx
    /// - make sure `idx` actually exists on the stack (gettop)
    unsafe fn get(state: *mut ffi::lua_State, idx: c_int, function_name: &'static str) -> Result<&'static Self, c_int>{
        let ud_ptr = unsafe {
            if ffi::lua_type(state, idx) == ffi::LUA_TUSERDATA {
                // SAFETY: ptr is null if ud is not a WebviewIpc; we check that the ptr is non-null later
                let ptr = ffi::lua_touserdatatagged(state, idx, WEBVIEW_IPC_TAG);
                // pop userdata off stack
                ffi::lua_remove(state, idx);
                ptr
            } else if ffi::lua_isnone(state, idx) == 1 {
                push_wrapped_error(state, &format!("{}: you forgot to pass self", function_name));
                return Err(1);
            } else {
                let got_t = utils::type_of(state, idx);
                // pop whatever we got to balance stack
                ffi::lua_remove(state, idx);
                push_wrapped_error(state, &format!("{}: expected to be passed self, got {}", function_name, got_t));
                return Err(1);
            }
        };

        if ud_ptr.is_null() {
            push_wrapped_error(state, "self is the wrong kind of userdata (expected WebviewIpc tag 13)");
            return Err(1);
        }
        
        // SAFETY: 
        // - The UserData pointer was created from Box<WebviewIpc>
        // - The UserData pointer tag was checked to be of type WebViewIpc
        // - The UserData pointer was checked to be non-null
        // - The WebviewIpc is owned by Rust and was leaked by Box::into_raw
        // - The caller is responsible for ensuring the WebviewIpc is leaked or otherwise still alive
        unsafe { 
            // first deref the ud pointer to get a pointer to WebviewIpc
            let ipc_ptr: *mut WebviewIpc = *(ud_ptr as *mut *mut WebviewIpc);
            if ipc_ptr.is_null() {
                push_wrapped_error(state, "inner pointer to *mut WebviewIpc inside the userdata holding *mut *mut WebviewIpc is null");
                return Err(1);
            }
            // next, deref the ipc ptr to get the actual WebviewIpc
            Ok(&*ipc_ptr)
        }
    }
    pub unsafe extern "C-unwind" fn replace_html(state: *mut ffi::lua_State) -> c_int {
        // index -2: userdata that stores *mut *mut WebviewIpc, index -1: new html to replace with

        let function_name = "WebviewIpc:replace_html(new_html: string)";

        let top = unsafe { ffi::lua_gettop(state) };
        if top != 2 {
            push_wrapped_error(state, &format!("{}: called without required arguments; expected 2 arguments (self, string), got {}", function_name, top));
            return 1;
        }

        // SAFETY: idx -2 is the correct idx; 2 elements are expected to be passed to this function
        let ipc = match unsafe { Self::get(state, -2, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        let new_html = unsafe {
            if ffi::lua_type(state, -1) == ffi::LUA_TSTRING {
                let ptr = ffi::lua_tostring(state, -1);
                let s = BString::clone_from_ptr(ptr).to_str_lossy().to_string();
                ffi::lua_pop(state, 1);
                s
            } else if ffi::lua_isnone(state, -1) == 1 {
                push_wrapped_error(state, &format!("{}: called without required argument new_html", function_name));
                return 1;
            } else {
                let got_t = utils::type_of(state, -1);
                // pop whatever we got to balance stack
                ffi::lua_pop(state, 1);
                push_wrapped_error(state, &format!("{}: expected 'new_html' to be a string, got {}", function_name, got_t));
                return 1;
            }
        };

        if let Err(err) = ipc.sender.send(ToWindow::ReplaceHtml(new_html)) {
            push_wrapped_error(state, &format!("unable to send message due to err: {}", err));
            return 1;
        }

        0
    }
    pub unsafe extern "C-unwind" fn try_read(state: *mut ffi::lua_State) -> c_int {
        // index -1: WebviewIpc userdata

        let function_name = "WebviewIpc:try_read(new_html: string)";

        let top = unsafe { ffi::lua_gettop(state) };
        if top != 1 {
            push_wrapped_error(state, &format!("{}: called without required arguments; expected 1 (self), got {}", function_name, top));
            return 1;
        }

        let ipc = match unsafe { Self::get(state, -1, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        match ipc.receiver.try_recv() {
            Ok(ToLuau::IpcMessage(message)) => {
                let message = match CString::new(message) {
                    Ok(s) => s,
                    Err(err) => {
                        let pos = err.nul_position();
                        CString::new(format!("{}: IPC message contains NUL byte at {}", function_name, pos)).unwrap()
                    }
                };

                unsafe { ffi::lua_pushstring(state, message.as_ptr()) };
            },
            Ok(ToLuau::WindowClosed) => {
                push_wrapped_error(state, "the window has been closed");
            },
            Ok(ToLuau::SizeReturned(_, _)) => unreachable!("only reachable from WindowIpc:size()"),
            Err(TryRecvError::Disconnected) => {
                push_wrapped_error(state, "channel is disconnected");
            },
            Err(TryRecvError::Empty) => {
                unsafe { ffi::lua_pushnil(state) };
            }
        }
        
        1
    }
     pub unsafe extern "C-unwind" fn alert(state: *mut ffi::lua_State) -> c_int {
        // WebviewIpc at idx -2, bool at idx -1
        let function_name = "WebviewIpc:alert(enabled: boolean)";
        let top = unsafe { ffi::lua_gettop(state) };
        if top != 2 {
            push_wrapped_error(state, &format!("{}: expected to be called with 2 arguments, got {}", function_name, top));
            return 1;
        }

        // SAFETY: -2 is the correct index
        let ipc = match unsafe { Self::get(state, -2, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        let enabled_type = unsafe { ffi::lua_type(state, -1) };
        let enabled = if enabled_type == ffi::LUA_TBOOLEAN {
            let b = unsafe { ffi::lua_toboolean(state, -1) };
            match b {
                0 => false,
                1 => true,
                _ => unreachable!("booleans are broken again")
            }
        } else {
            push_wrapped_error(state, &format!("{}: expected enabled to be a boolean, got something else or nil", function_name));
            return 1;
        };

        if let Err(err) = ipc.sender.send(ToWindow::SetAlert(enabled)) {
            push_wrapped_error(state, &format!("{}: unable to send message via ipc due to err: {}", function_name, err));
            return 1;
        }

        0
    }
    pub unsafe extern "C-unwind" fn size(state: *mut ffi::lua_State) -> c_int {
        // self should be at idx -1
        let function_name = "WebviewIpc:size";

        let top = unsafe { ffi::lua_gettop(state) };
        if top != 1 {
            push_wrapped_error(state, &format!("{}: called without required arguments; expected 1 (self), got {}", function_name, top));
            return 1;
        }

        // SAFETY: idx -1 is the correct idx for only self fn
        let ipc = match unsafe { Self::get(state, -1, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        if let Err(err) = ipc.sender.send(ToWindow::SizeRequested) {
            push_wrapped_error(state, &format!("{}: unable to send request for size due to err {}", function_name, err));
            return 1;
        };

        match ipc.receiver.recv() {
            Ok(ToLuau::SizeReturned(width, height)) => {
                unsafe { ffi::lua_pushvector(state, width, height, 0.0) };
            },
            Ok(t) => {
                push_wrapped_error(state, &format!("{}: unexpected message type returned: {:?}", function_name, t));
                return 1;
            }
            Err(err) => {
                push_wrapped_error(state, &format!("{}: unable to recv due to err: {}", function_name, err));
                return 1;
            }
        };

        1
    }
    pub unsafe extern "C-unwind" fn close(state: *mut ffi::lua_State) -> c_int {
        // WebviewIpc should be at stack index -1

        let function_name = "WebviewIpc:close()";

        let top = unsafe { ffi::lua_gettop(state) };
        if top != 1 {
            push_wrapped_error(state, &format!("{}: expected to be called with only self, got {} arguments", function_name, top));
            return 1;
        }

        let ipc = match unsafe { Self::get(state, -1, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        if let Err(err) = ipc.sender.send(ToWindow::Close) {
            push_wrapped_error(state, &format!("{}: unable to send message to close window due to err: {}", function_name, err));
            return 1;
        }

        0
    }

}