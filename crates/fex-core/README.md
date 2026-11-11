# fex-core

The parts of Plume FEx that have no interface, and their tests.

- `listing`: reads a folder in one or a few system calls (names, sizes, dates, attributes, NTFS file IDs).
- `sort`: Explorer's order (natural numbers, accents next to their letter), by name, size, date or type.
- `index`: every file and folder of the fixed drives in flat arrays, about 50 bytes each; built by a parallel
  walk, kept current from the list of folders that changed, saved to and loaded from one file.
- `usn`: the NTFS change journal, read without administrator rights.
- `query`: Everything's search syntax (words, phrases, wildcards, regular expressions, filters, duplicates…)
  and the search itself, spread over the processor's threads.

```bash
cargo test -p fex-core
```

`examples/fexbench.rs` measures a real drive: `cargo run --release -p fex-core --example fexbench -- index C:\ -- report "ext:mp4 size:>100mb"`.
