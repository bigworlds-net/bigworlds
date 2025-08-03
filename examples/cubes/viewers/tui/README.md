# `cubes` TUI viewer

Simple terminal-based 2D viewer for the `cubes` model.

Position of the cubes will be shown on xy and xz planes as little dots on the
canvas.


## Running

Navigate to the viewer project directory and execute `cargo run --release` in
your favourite terminal.

Viewer will attempt to connect to a server at `127.0.0.1:9123`. You can provide
a custom server address as an argument to the executable, e.g.
`cargo run --release -- 127.0.0.1:9999`

## TODO

Put some more work into rendering the cubes correctly onto ratatui viewports.
Currently we're casting the 3D position values and using that when drawing on
the canvas, just to have something to put on the screen.

Would be nice to make color of the individual dot represents the worker
currently holding the cube entity.

