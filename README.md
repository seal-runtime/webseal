# webseal

Tao/Wry webview bindings for [seal](https://github.com/seal-runtime/seal)

This only really has been 'tested' on Linux, but *should* work on Windows.

You'll need a version of *seal* that exposes bindings to the C-Stack API for `sealbindings`.

## Usage

```luau

local window = webseal.create {
    title = "yourtitle",
    min_size = vector.create(400, 400),
    size = vector.create(600, 400),
    html = your_html,
}

-- webview runs in another thread, so you must keep
-- the main application alive in a loop to keep the
-- webview from exiting
while time.wait(0.25) do
    local message = window:try_read()
    if message then
        print(message)
        if message == "next page" then
            window:replace_html(next_page_content)
        end
    end
end
```

The program exits when the webview exits. I'm investigating ways to use `run_return` to avoid that
but it's not as simple as expected.

## Building

Run `seal r` in this repository to execute `./.seal/build.luau`. You'll need Rust
and most likely, you'll need to install whatever Wry/Tao bind to under the hood.

Once built, you should probably copy/move `./crate` somewhere you typically keep dependencies like these,
near your `~` so you can easily add it as an alias in your `.luaurc` config file to require from *seal*.
