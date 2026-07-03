local extern = require("@interop/extern")
local env = require("@std/env")
local fs = require("@std/fs")

export type WebviewOptions = {
    --- Defines the window manager title for the window as well as the visual title in the titlebar, if shown.
    title: string,
    --- Show the titlebar as part of the actual html? If disabled, doesn't show the close/minimize/etc. buttons as well.
    --- Defaults to `true`.
    show_titlebar: boolean?,
    html: string,
    --- starting size of the window
    size: vector?,

    min_size: vector?,
    max_size: vector?,

    resizeable: boolean?,
    always_on_top: boolean?,
}

type WebviewIpc = {
    replace_html: (self: WebviewIpc, html: string) -> (),
    try_read: (self: WebviewIpc) -> string?,
    alert: (self: WebviewIpc, enabled: boolean) -> (),
    size: (self: WebviewIpc) -> vector,
    close: (self: WebviewIpc) -> (),
}

export type webseal = {
    create: (options: WebviewOptions) -> WebviewIpc,
}

local target_os, target_arch = env.os:lower(), env.arch

local binary: string? = fs.listdir(fs.path.join(script:parent(), "bins"), false, function(path)
    if string.match(path, target_os) and string.match(path, target_arch) then
        return true
    end
    return false
end)[1]

if not binary then
    error(`Your target os/architecture is not supported. Please compile the bin yourself or make an issue for support.`)
end

return extern.load(binary) :: webseal