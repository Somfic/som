# Corpus

Every som program the compiler is tested against. Each case is either a single
`<category>/<name>.som` file, or a `<category>/<name>/` directory with a
`main.som` for multi-file projects.

The header comments at the top of the case (or its `main.som`) say what
should happen:

```som
// expect: exit 42                     compiles, runs, and exits with 42
// expect: ok                          passes every check (not run; may have no main)
// expect: error                       fails with at least one error
// expect: error Mismatch InvalidOperands  fails with at least these error kinds
// ignore: reason                      known failure, skipped by default
```

The `som_corpus` crate (in `tests/`) turns every case into a `#[test]` at build time:

```sh
cargo test -p som_corpus                     # run everything
cargo test -p som_corpus -- borrow_check::   # one category
cargo test -p som_corpus -- --ignored        # only the known failures
```

Adding a test means adding a file; no Rust needed.
