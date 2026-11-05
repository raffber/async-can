# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - unreleased

## [Unreleased]

### Added

- `socketcan::connect()`, which opens a socket and splits it into a `socketcan::Sender` and `socketcan::Receiver`, in line with the other backends. Frames sent through the `Sender` are not received by its `Receiver`.

### Changed

- **Breaking:** Minimum supported Rust version is now 1.88, and the crate uses edition 2024.
- **Breaking:** `socketcan::CanSocket` no longer implements `mio::event::Source`. This removes `mio` 0.7 from the public API.
- PCAN bindings are pre-generated and committed, so building no longer requires `bindgen` or libclang. Regenerate them with `scripts/gen-pcan-bindings.sh`.
- SocketCAN uses the `libc` definitions of `can_frame` and `sockaddr_can` instead of hand-written structs.
- Updated dependencies: `rtnetlink` 0.24, `windows-sys` 0.61, `thiserror` 2. The unmaintained `dlopen` crate is replaced with `libloading`, and `lazy_static` with `std::sync::LazyLock`.
- Added crates.io metadata to `Cargo.toml`.
- **Breaking:** New `Error::PCanLibraryLoad` variant.

### Fixed

- `socketcan::CanSocket` no longer leaks its file descriptor if binding the socket fails.
- `socketcan::CanSocket` no longer reads an uninitialized frame if `read()` fails.
- PCAN functions return `Error::PCanLibraryLoad` instead of panicking if the PCAN library cannot be loaded, e.g. on non-x86-64 Linux or when the temp directory is not writable or mounted `noexec`.
- `usr_canet::Receiver` returns `Error::DataTooLong` instead of panicking on frames with a DLC above 8.
- Dropping a `pcan::Receiver` on Linux now stops its receive thread even if no further frames arrive.
- The PCAN receive thread on Linux no longer exits when `poll()` is interrupted by a signal.
- Building with the `pcan` feature on 32-bit Linux.
- PCAN error descriptions in non-UTF-8 system languages no longer cause a panic.
- On Windows, `pcan::Receiver::connect()` returns an error instead of panicking if the receive event cannot be created or registered, and a failed wait ends the receiver with an error instead of a panic.

## [0.3.0] - 2023-01-11

### Added

- `loopback` module: an in-memory sender/receiver pair for testing.
- Cargo features `pcan`, `socket_can` and `usr_canet` to enable each backend individually.
- `DataFrame::take_data()` to move the payload out of a frame.
- `socketcan::CanSocket::try_clone()`.
- Continuous integration (build, test, clippy and rustfmt).

### Changed

- **Breaking:** `socketcan::Sender` and `socketcan::Receiver` are removed. `socketcan::CanSocket` implements the `Sender` and `Receiver` traits directly, and its `send()`/`recv()` methods are public and return `io::Result`.
- `usr_canet` sets `TCP_NODELAY` on its connection.
- `bindgen` only runs if the `pcan` feature is enabled.

### Fixed

- Stack overflow when sending or receiving through the SocketCAN `Sender`/`Receiver` traits.
- Wake-up handling of the PCAN receiver.
- Undefined behavior from reading uninitialized memory in the PCAN bindings.

## [0.2.0] - 2022-08-23

Initial release.

### Added

- Tokio-based `Sender` and `Receiver` traits, with `Message`, `DataFrame` and `RemoteFrame` types that are validated on construction and deserialization.
- SocketCAN support on Linux, including listing interfaces and setting them up or down via netlink.
- PCAN support on Windows and Linux through PCAN-Basic, with event-based receiving, device listing and re-initialization with a different bitrate.
- USR-CANET200 support (`usr_canet` module).
- Optional `serde` feature.

[Unreleased]: https://github.com/raffber/async-can/compare/40f980d...HEAD
[0.3.0]: https://github.com/raffber/async-can/compare/b7f30b9...40f980d
[0.2.0]: https://github.com/raffber/async-can/commits/b7f30b9
