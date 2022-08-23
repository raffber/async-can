# async-can - Asynchronous CAN Stack running on tokio

Library to connect to CAN buses. Currently supports:

 * `SocketCAN` on linux-only
 * `PCAN` devices from [Peak Systems](https://www.peak-system.com)
 * `USR-CANET200` TCP protocol from [USR IOT](https://www.pusr.com/)

This library has been tested on Linux and Windows.
Additionally this library supports enumerating CAN devices connected to a host.

## Roadmap

This library is far from feature-complete. The following provides a list of features that are implemented / on the roadmap (roughly in the order of priority):

- [x] Basic CAN message exchange on all supported interfaces
- [x] Listing connected CAN devices / adapters
- [ ] Allow chaning SocketCAN adapter settings (currently only supported to set interface up and down)
- [ ] Support for PCAN devices not connected over USB or PCI-E
- [ ] Get real hardware timestamps for SocketCAN with `netlink` sockets
- [ ] Support for CAN-FD

PRs are very much welcome for all those features or anything else related.

## License

Licensed under either of

- Apache License, Version 2.0, (LICENSE-APACHE or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license (LICENSE-MIT or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.