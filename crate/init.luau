local extern = require("@interop/extern")
local env = require("@std/env")
local fs = require("@std/fs")

export type WebviewOptions = {
    title: string,
    html: string,
    size: vector?,
    resizeable: boolean?,
    min_size: vector?,
    max_size: vector?,
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