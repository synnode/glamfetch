# Bundled FIGlet fonts

These `.flf` files ship with glamfetch and are embedded into the binary
via `include_str!`. They originate from the standard FIGlet font
collection, which is freely redistributable per the FIGlet license
(<http://www.figlet.org/>).

Each file retains its original header attribution to the font author.

The `font = "<name>"` widget parameter looks up these names
case-insensitively, with or without the `.flf` extension. Anything
containing `/` or starting with `~` is treated as a filesystem path
instead.
