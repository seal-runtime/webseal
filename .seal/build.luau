local env = require("@std/env")
local fs = require("@std/fs")
local process = require("@std/process")

local BUILD: "debug" | "release" = "release"

local target_os, target_arch = env.os:lower(), env.arch
local tag = `{target_os}-{target_arch}`

print("Compiling binary package...")
process.run {
    program = "cargo",
    args = if BUILD == "debug" then { "b" } else { "b", "--release" },
    stdio = "Inherit",
}:unwrap()

print("Compiled binary cdylib")

local binary_name = 
    if env.os == "Windows" then 
        "webseal.dll"
    elseif env.os == "Linux" then 
        "libwebseal.so" 
    else
        error("unsupported os")

local crate_dir = fs.dir.project():join("crate")
local bins_path = fs.path.join(crate_dir, "bins")

local bins = fs.dir.ensure(bins_path)
local new_name = `webseal-{tag}.{if env.os == "Windows" then "dll" else "so"}`

fs.copy(fs.dir.project():join("target", BUILD, binary_name), bins:join(new_name))

print(`Moved {new_name} to bins path`)

local seal_dir = env.vars.get("SEAL_DIR") or fs.dir.home():join(".seal")
local webseal_path = fs.path.join(seal_dir, "shared", "webseal")
if fs.path.exists(webseal_path) then
    fs.removetree(webseal_path)
    fs.copy(crate_dir, webseal_path)
    print("Updated ~/.seal/shared/webseal")
end
