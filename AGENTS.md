# AGENTS.md

`async-can` is an asynchronous CAN bus stack on tokio, supporting SocketCAN, PCAN and USR-CANET200.

## Workflow

Use the Makefile targets (`make help` lists them):

- `make format`: format the code with `cargo fmt`
- `make check`: spell-check with cspell, then `cargo check` and `cargo clippy`
- `make build`: build all features and targets
- `make test`: run the tests
- `make bindgen`: regenerate `src/pcan/bindings.rs` from `include/PCANBasic.h`

## Checking your work

Before you call a change done, run `make format`, `make check` and `make test`, and fix anything they report. CI treats warnings as errors (`RUSTFLAGS=-D warnings`), so fix clippy warnings too.

If cspell flags a real word, such as an identifier or an API name, add it to the `words` list in `cspell.json`, keeping the list in alphabetical order.

Never edit `src/pcan/bindings.rs` by hand. Regenerate it with `make bindgen`.
