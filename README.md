# cols

A faster `colcon list` replacement for fuzzy-finders.

## Installation

```bash
cargo install --path .
```

The executable will be installed to `~/.cargo/bin/cols`.
To run the executable without specifying the full path, `~/.cargo/bin` must be added to the `$PATH`.

## Usage

```console
$ cols list
test_cmake_pkg  ./test_tree/subrepo/another_package     (ament_cmake)
test_python_pkg ./test_tree/subrepo/python_package      (ament_python)
zzz_package     ./test_tree/a_package   (ament_cmake)

$ cols list --base-paths test_tree/subrepo --names-only
test_cmake_pkg
test_python_pkg
```

For more options, see `cols list --help`.

## Limitations

- Only implements the basic list options
- Package paths may be printed as absolute paths and sorting may not be identical

## Fuzzy-finder examples

Change directory to a package picked by fzf:

```bash
function rcd {
  local pkgpath
  pkgpath=$(cols list | fzf -q "${1}" | cut -f 2)
  [ -n "${pkgpath}" ] && cd "${pkgpath}"
}
```

`rosed` replacement:

```bash
function re {
  local pkg
  pkg=$(cols list | fzf -q "${1}" | cut -f 2)
  [ -n "${pkg}" ] || return
  local filename
  filename=$(fd -0 --type f . "${pkg}" | fzf --read0 --prompt="file: ")
  [ -n "${filename}" ] || return
  ${EDITOR} "${filename}"
}
```

## Usage with telescope-ros.nvim

`cols` can act as a drop-in replacement for `colcon` in [telescope-ros.nvim](https://github.com/bi0ha2ard/telescope-ros.nvim):

```lua
require('telescope').setup{
    extensions = {
        ros = {
            colcon = "cols" -- Must be in $PATH
            -- colcon = vim.loop.os_homedir() .. "/.cargo/bin/cols" -- works too
        }
    }
}
```
