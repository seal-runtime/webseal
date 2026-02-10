use std::cell::RefCell;

use std::ffi::c_int;

pub mod utils;
pub mod options;

mod webview_ipc;
mod resize;

use webview_ipc::WebviewIpc;

use seal::{ffi, push_wrapped_c_function, push_wrapped_error};

use options::WebviewOptions;

use tao::{
    dpi::LogicalSize, event::{Event, StartCause, WindowEvent}, 
    event_loop::{ControlFlow, EventLoopBuilder}, 
    platform::unix::EventLoopBuilderExtUnix, 
    window::{UserAttentionType, WindowBuilder}
};
use wry::{WebViewBuilder, http::Request};
use tao::platform::unix::WindowExtUnix;

use crate::resize::HitTestResult;

enum UserEvent {
    Minimize,
    Maximize,
    DragWindow,
    CloseWindow,
    MouseDown(i32, i32),
    MouseMove(i32, i32),
    SendIpc(String),
}

#[derive(Debug)]
pub enum ToLuau {
    IpcMessage(String),
    SizeReturned(f32, f32),
    WindowClosed,
}

#[derive(Debug)]
pub enum ToWindow {
    ReplaceHtml(String),
    SetAlert(bool),
    SizeRequested,
    Close,
}

fn spawn(options: WebviewOptions, sender: crossbeam_channel::Sender<ToLuau>, receiver: crossbeam_channel::Receiver<ToWindow>) -> wry::Result<()> {
    let event_loop = EventLoopBuilder::<UserEvent>
        ::with_user_event()
        .with_any_thread(true)
        .build();

    let builder = WindowBuilder::new()
        .with_decorations(false)
        .with_transparent(true)
        .with_title(&options.title)
        .with_inner_size(LogicalSize::new(options.size.0, options.size.1))
        .with_resizable(options.resizeable);

    // why do none of these methods take in &self instead of the whole self
    let builder = if let Some(max_size) = options.max_size {
        builder.with_max_inner_size(LogicalSize::new(max_size.0, max_size.1))
    } else { 
        builder 
    };

    let builder = if let Some(min_size) = options.min_size {
        builder.with_min_inner_size(LogicalSize::new(min_size.0, min_size.1))
    } else {
        builder
    };

    let window = builder
        .build(&event_loop)
        .unwrap();

    let handler_proxy = event_loop.create_proxy();
    let handler = move |req: Request<String>| {
        let body = req.body();
        if body.starts_with("input!") {
            let body = body.replace("input!", "");
            let mut req = body.split([':', ',']);
            match req.next().unwrap() {
                "minimize" => {
                    let _ = handler_proxy.send_event(UserEvent::Minimize);
                }
                "maximize" => {
                    let _ = handler_proxy.send_event(UserEvent::Maximize);
                }
                "drag_window" => {
                    let _ = handler_proxy.send_event(UserEvent::DragWindow);
                }
                "close" => {
                    let _ = handler_proxy.send_event(UserEvent::CloseWindow);
                }
                "mousedown" => {
                    let x = req.next().unwrap().parse().unwrap();
                    let y = req.next().unwrap().parse().unwrap();
                    let _ = handler_proxy.send_event(UserEvent::MouseDown(x, y));
                }
                "mousemove" => {
                    let x = req.next().unwrap().parse().unwrap();
                    let y = req.next().unwrap().parse().unwrap();
                    let _ = handler_proxy.send_event(UserEvent::MouseMove(x, y));
                }
                _ => {}
            }
        } else {
            let _ = handler_proxy.send_event(UserEvent::SendIpc(body.clone()));
            // let _ = handler_proxy.send_event(UserEvent::InvokeLuauCallback(body.clone()));
        }
    };

    const HTML_HEADER: &str = include_str!("./template.html");

    let html = HTML_HEADER.replace("!REPLACETITLE!", &options.title);
    let html = html.replace("!REPLACEBODY!", &options.html);

    let builder = WebViewBuilder::new()
        .with_html(html)
        .with_transparent(true)
        .with_ipc_handler(handler)
        .with_accept_first_mouse(true);

    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    ))]
    let webview = builder.build(&window)?;
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        target_os = "ios",
        target_os = "android"
    )))]
    let webview = {
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().unwrap();
        builder.build_gtk(vbox)?
    };

    // let mut webview = Some(webview);
    let webview = RefCell::new(webview);
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;

        let mut new_html: Option<String> = None;
        let mut set_alert: Option<bool> = None;
        match receiver.try_recv() {
            Ok(ToWindow::ReplaceHtml(html)) => {
                new_html = Some(html);
            },
            Ok(ToWindow::SetAlert(enabled)) => {
                set_alert = Some(enabled);
            },
            Ok(ToWindow::Close) => {
                *control_flow = ControlFlow::Exit;
            },
            Ok(ToWindow::SizeRequested) => {
                let size = window.inner_size();
                let width = size.width as f32;
                let height = size.height as f32;
                if let Err(err) = sender.send(ToLuau::SizeReturned(width, height)) {
                    eprintln!("error reporting size to luau: {}", err);
                }
            }
            _ => {}
        }

        if let Some(new_html) = new_html {
            let webview = webview.borrow_mut();
            let html = HTML_HEADER.replace("!REPLACETITLE!", &options.title);
            let html = html.replace("!REPLACEBODY!", &new_html);
            let _ = webview.load_html(&html);
        }

        if let Some(should_set_alert) = set_alert {
            // let webview = webview.borrow_mut();
            if should_set_alert {
                window.request_user_attention(Some(UserAttentionType::Critical));
            } else {
                window.request_user_attention(None);
            }
        }

        match event {
            Event::NewEvents(StartCause::Init) => {},
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            }
            | Event::UserEvent(UserEvent::CloseWindow) => {
                if let Err(err) = sender.send(ToLuau::WindowClosed) {
                    eprintln!("can't tell luau we closed window due to err: {}", err);
                }
                let _ = webview.borrow_mut();
                *control_flow = ControlFlow::Exit
            }
            Event::UserEvent(e) => match e {
                UserEvent::Minimize => window.set_minimized(true),
                UserEvent::Maximize => window.set_maximized(!window.is_maximized()),
                UserEvent::DragWindow => window.drag_window().unwrap(),
                UserEvent::MouseDown(x, y) => {
                    let res = resize::check_bounds(window.inner_size(), x, y, window.scale_factor());
                    match res {
                        HitTestResult::Client | HitTestResult::NoWhere => {}
                        _ => res.drag_resize_window(&window),
                    }
                }
                UserEvent::MouseMove(x, y) => {
                    resize::check_bounds(window.inner_size(), x, y, window.scale_factor())
                        .change_cursor(&window);
                }
                UserEvent::CloseWindow => { /* handled above */ },
                UserEvent::SendIpc(body) => {
                    if let Err(err) = sender.send(ToLuau::IpcMessage(body)) {
                        eprintln!("unable to send ipc message due to err: {}", err);
                    }
                }
            },
            _ => (),
        }

        // prevent busy waiting due to controlflow Poll
        // std::thread::sleep(std::time::Duration::from_millis(20));
    });
    // Ok(())
}

unsafe extern "C-unwind" fn webview_create(state: *mut ffi::lua_State) -> c_int {
    let function_name = "webview.create(options: WebviewOptions)";

    let top = unsafe { ffi::lua_gettop(state) };
    if top != 1 {
        push_wrapped_error(state, &format!("{}: incorrect number of arguments passed; expected 1 argument (table), got: {}", function_name, top));
    }

    let t = unsafe { utils::type_of(state, -1) };
    if t != b"table" {
        push_wrapped_error(state, &format!("{}: expected table, got: {}", function_name, t));
        return 1;
    }

    // SAFETY: state must be valid, stack top is luau table
    let options = match unsafe { WebviewOptions::from_table_on_stack(state, function_name) } {
        Ok(options) => options,
        Err(error_code) => { // error message is pushed to stack
            return error_code;
        }
    };

    let (to_luau_tx, to_luau_rx) = crossbeam_channel::unbounded::<ToLuau>();
    let (to_window_tx, to_window_rx) = crossbeam_channel::unbounded::<ToWindow>();

    std::thread::spawn(|| {
        if let Err(err) = spawn(options, to_luau_tx, to_window_rx) {
            eprintln!("webview.create: unable to spawn webview due to err: {}", err);
        }
    });

    let handler = Box::new(WebviewIpc {
        sender: to_window_tx,
        receiver: to_luau_rx,
    });

    let boxed = Box::into_raw(handler);

    unsafe {
        ffi::luaL_checkstack(state, 6, c"can't stack".as_ptr());
        ffi::lua_createtable(state, 0, 6);

        ffi::lua_pushvalue(state, -1); // copy table val so index points to itself and doesnt get self popped
        ffi::lua_setfield(state, -2, c"__index".as_ptr()); // __index should point to itself

        push_wrapped_c_function(state, WebviewIpc::replace_html);
        ffi::lua_setfield(state, -2, c"replace_html".as_ptr());

        push_wrapped_c_function(state, WebviewIpc::try_read);
        ffi::lua_setfield(state, -2, c"try_read".as_ptr());

        push_wrapped_c_function(state, WebviewIpc::close);
        ffi::lua_setfield(state, -2, c"close".as_ptr());

        push_wrapped_c_function(state, WebviewIpc::alert);
        ffi::lua_setfield(state, -2, c"alert".as_ptr());

        push_wrapped_c_function(state, WebviewIpc::size);
        ffi::lua_setfield(state, -2, c"size".as_ptr());

        ffi::lua_pushstring(state, c"WebviewIpc".as_ptr());
        ffi::lua_setfield(state, -2, c"__type".as_ptr()); // typeof(ud)

        ffi::lua_setuserdatametatable(state, 13);

        let ud = ffi::lua_newuserdatataggedwithmetatable(
            state, 
            std::mem::size_of::<*mut WebviewIpc>(), 
            webview_ipc::WEBVIEW_IPC_TAG
        ) as *mut *mut WebviewIpc;
        // write the pointer *mut WebviewIpc into userdata that stores *mut *mut WebviewIpc
        *ud = boxed;
    }

    // WebviewIpc userdata left on stack

    1
}

/// The entrypoint to an extern library/plugin for the seal runtime.
/// 
/// This function must return one value on the Luau stack,
/// usually a table (usually of functions) exposed by this library.
/// 
/// # Safety
/// - Caller must pass a valid, non-null pointer to a lua_State.
/// - This library must use sealbindings or equivalent to access *seal*'s exposed
///   C-stack API, and should not bind to Luau separately.
/// - This library *must* be kept alive by *seal* (or the caller) for 'static (forever).
///   If the library is prematurely closed, or functions from this library
///   are dropped, subsequent calls to those functions from Luau WILL cause segfaults and/or UB.
///   In Rust, use `std::mem::ManuallyDrop` to keep a libloading Library alive for longer than the function call.
/// - This function must call `sealbindings::initialize()` immediately.
#[unsafe(no_mangle)]
pub unsafe extern "C-unwind" fn seal_open_extern(state: *mut ffi::lua_State) -> c_int {
    unsafe {
        seal::initialize();

        ffi::lua_createtable(state, 0, 0);

        push_wrapped_c_function(state, webview_create);
        ffi::lua_setfield(state, -2, c"create".as_ptr());

        // table left on stack
    }
    1
}
