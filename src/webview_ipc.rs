use std::ffi::c_int;
use std::sync::atomic::AtomicBool;

use crossbeam_channel::TryRecvError;
use sealbindings::{LuauState, SealValue, StateExt};
use crate::{ToLuau, ToWindow};

pub const WEBVIEW_IPC_TAG: c_int = 13;
pub static WEBVIEW_METATABLE_IS_SET_UP: AtomicBool = AtomicBool::new(false);

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
    unsafe fn pop(state: *mut LuauState, idx: c_int, function_name: &'static str) -> Result<&'static Self, c_int>{
        let ud_ptr: *mut *mut WebviewIpc = match state.to_seal(idx) {
            SealValue::UserData { tag, ptr, .. } => {
                if tag == WEBVIEW_IPC_TAG {
                    // we know this must be a WebViewIpc
                    ptr as *mut *mut WebviewIpc
                } else {
                    return Err(state.push_wrapped_error("self is the wrong kind of userdata (expected WebviewIpc)"));
                }
            },
            other => {
                return Err(state.push_wrapped_error(format!("{} expected self to be a WebviewIpc userdata, got: {:?}", function_name, other)));
            }
        };

        if ud_ptr.is_null() {
            return Err(state.push_wrapped_error("ud_ptr is null; this shouldn't really happen"));
        }

        // SAFETY: 
        // - The UserData pointer was created from Box<WebviewIpc>
        // - The UserData pointer tag was checked to be of type WebViewIpc
        // - The UserData pointer was checked to be non-null
        // - The WebviewIpc is owned by Rust and was leaked by Box::into_raw
        // - The caller is responsible for ensuring the WebviewIpc is leaked or otherwise still alive
        let ipc = unsafe { 
            // first deref the ud pointer to get a pointer to WebviewIpc
            let ipc_ptr: *mut WebviewIpc = *ud_ptr;
            if ipc_ptr.is_null() {
                return Err(
                    state.push_wrapped_error("inner pointer to *mut WebviewIpc inside the userdata holding *mut *mut WebviewIpc is null")
                );
            }
            // next, deref the ipc ptr to get the actual WebviewIpc
            &*ipc_ptr
        };

        // since we're successful, remove the userdata off stack
        unsafe { state.remove(idx) };

        Ok(ipc)
    }
    pub unsafe extern "C-unwind" fn replace_html(state: *mut LuauState) -> c_int {
        // index -2: userdata that stores *mut *mut WebviewIpc, index -1: new html to replace with

        let function_name = "WebviewIpc:replace_html(new_html: string)";

        let _sg = state.stack_changes(-1);

        // SAFETY: idx -2 is the correct idx; 2 elements are expected to be passed to this function
        // Self::pop automatically handles cases where arguments are not as expected
        let ipc = match unsafe { Self::pop(state, -2, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        let new_html = match state.to_seal(-1) {
            SealValue::String(s) => s.to_string(),
            SealValue::None => {
                return state.push_wrapped_error(format!("{} called without required argument new_html (expected string)", function_name));
            },
            other => {
                return state.push_wrapped_error(format!("{}: expected new_html to be a string, got: {:?}", function_name, other));
            }
        };

        if let Err(err) = ipc.sender.send(ToWindow::ReplaceHtml(new_html)) {
            return state.push_wrapped_error(format!("unable to send message due to err: {}", err));
        }

        0
    }
    pub unsafe extern "C-unwind" fn try_read(state: *mut LuauState) -> c_int {
        // index -1: WebviewIpc userdata

        let function_name = "WebviewIpc:try_read()";

        let _sg = state.stack_returns_or_errs(1);

        // Self::pop automatically handles cases where arguments are not as expected
        let ipc = match unsafe { Self::pop(state, -1, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        match ipc.receiver.try_recv() {
            Ok(ToLuau::IpcMessage(message)) => {
                state.push_str(message);
            },
            Ok(ToLuau::WindowClosed) => {
                return state.push_wrapped_error("the window has been closed");
            },
            Ok(ToLuau::SizeReturned(_, _)) => unreachable!("only reachable from WindowIpc:size()"),
            Err(TryRecvError::Disconnected) => {
                return state.push_wrapped_error("channel is disconnected");
            },
            Err(TryRecvError::Empty) => {
                state.push_nil();
            }
        }
        
        1
    }
    pub unsafe extern "C-unwind" fn alert(state: *mut LuauState) -> c_int {
        // WebviewIpc at idx -2, bool at idx -1
        let function_name = "WebviewIpc:alert(enabled: boolean)";

        // SAFETY: -2 is the correct index
        let ipc = match unsafe { Self::pop(state, -2, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        let enabled = if state.is_boolean(-1) {
            // SAFETY: we know it's a boolean
            unsafe { state.to_boolean(-1) }
        } else {
            let what = state.to_seal(-1);
            return state.push_wrapped_error(format!("{}: expected 'enabled' to be a boolean, got {:?}", function_name, what));
        };

        if let Err(err) = ipc.sender.send(ToWindow::SetAlert(enabled)) {
            return state.push_wrapped_error(format!("{}: unable to send message via ipc due to err: {}", function_name, err));
        }

        0
    }
    pub unsafe extern "C-unwind" fn size(state: *mut LuauState) -> c_int {
        // self should be at idx -1
        let function_name = "WebviewIpc:size";

        let _sg = state.stack_returns_or_errs(1);

        let top = state.top();
        if top != 1 {
            return state.push_wrapped_error(format!("{}: called without required arguments; expected just self, got: {} arguments", function_name, top));
        }

        // SAFETY: idx -1 is the correct idx for only self fn
        let ipc = match unsafe { Self::pop(state, -1, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        if let Err(err) = ipc.sender.send(ToWindow::SizeRequested) {
            return state.push_wrapped_error(format!("{}: unable to request size due to err: {}", function_name, err));
        };

        match ipc.receiver.recv() {
            Ok(ToLuau::SizeReturned(width, height)) => {
                state.push_vector(width, height, 0.0);
            },
            Ok(t) => {
                return state.push_wrapped_error(format!("{}: unexpected message type returned: {:?}", function_name, t));
            }
            Err(err) => {
                return state.push_wrapped_error(format!("{}: unable to recv due to err: {}", function_name, err));
            }
        };

        1
    }
    pub unsafe extern "C-unwind" fn close(state: *mut LuauState) -> c_int {
        // WebviewIpc should be at stack index -1

        let function_name = "WebviewIpc:close()";

        let top = state.top();
        if top != 1 {
            return state.push_wrapped_error(format!("{}: expected to be called with only self, got {} arguments", function_name, top));
        }

        let ipc = match unsafe { Self::pop(state, -1, function_name) } {
            Ok(ipc) => ipc,
            Err(rets) => {
                return rets;
            }
        };

        if let Err(err) = ipc.sender.send(ToWindow::Close) {
            return state.push_wrapped_error(&format!("{}: unable to send message to close window due to err: {}", function_name, err));
        }

        0
    }

    pub unsafe extern "C-unwind" fn setup_metatable(state: *mut LuauState) -> c_int {
        // we dont return anything from this function, it just sets up the WebviewIpc metatable
        let _sg = state.stack_balanced();

        if WEBVIEW_METATABLE_IS_SET_UP.load(std::sync::atomic::Ordering::Acquire) {
            return 0;
        }

        // in modern luau versions, checkstack shouldn't be needed; this is just in case
        unsafe { state.ensure_stack(3, c"need 3 stack slots available".as_ptr()) };

        // metatable
        state.create_table(0, 6);

        unsafe {
            state.push_value(-1); // copy table ref so metatable doesn't get itself popped 
            state.set_field(-2, c"__index"); // __index should point to itself for perf
        }

        state.push_str("WebviewIpc");
        unsafe {
            // allow typeof(ud) == "WebviewIpc"
            state.set_field(-2, c"__type");

            state.set_wrapped_function(
                c"replace_html",
                WebviewIpc::replace_html,
                c"WebviewIpc:replace_html(html: string)"
            );

            state.set_wrapped_function(
                c"try_read",
                WebviewIpc::try_read,
                c"WebviewIpc:try_read() -> string?"
            );

            state.set_wrapped_function(
                c"close",
                WebviewIpc::close,
                c"WebviewIpc:close()"
            );

            state.set_wrapped_function(
                c"alert",
                WebviewIpc::alert,
                c"WebviewIpc:alert(enabled: boolean)",
            );

            state.set_wrapped_function(
                c"size",
                WebviewIpc::size,
                c"WebviewIpc:size() -> vector"
            );

            sealbindings::ffi::lua_setuserdatametatable(state, WEBVIEW_IPC_TAG);
        }

        WEBVIEW_METATABLE_IS_SET_UP.store(true, std::sync::atomic::Ordering::Relaxed);

        0
    }

    /// Takes in a raw pointer to the WebviewIpc that was previously Boxed and made raw via Box::into_raw.
    /// 
    /// # Safety
    /// - `ptr` must be valid and point to a leaked WebviewIpc
    pub unsafe fn from_raw(state: *mut LuauState, ptr: *mut WebviewIpc) {
        unsafe {
            state.ensure_stack(1, c"should have 1 space on stack".as_ptr());

            let ud = sealbindings::ffi::lua_newuserdatataggedwithmetatable(
                state,
                std::mem::size_of::<*mut WebviewIpc>(), 
                WEBVIEW_IPC_TAG
            ) as *mut *mut WebviewIpc;
            // write the pointer *mut WebviewIpc into userdata that stores *mut *mut WebviewIpc
            *ud = ptr;
        }

        // leaves userdata on stack
    }

}