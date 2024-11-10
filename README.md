# Daybreak

A Work-In-Progress Dawn replacement built in Rust for the TTY.

Daybreak is built as a Daemon, which allows for held connections to the Robot.

> [!NOTE]
> Development is heavily recommended to be done in the Nix Flake provided.

## Goals

- [x] Support the full Dawn Networking Stack*

> Technically, the entire Dawn Networking Stack hasn't been implemented. However, the most common features that are still being used (run mode, inputs, device list, upload/download) are supported.

- [x] Modularity

- [x] Support Input Methods
- [x] Support Keyboard Input
- [x] Upload code command
- [ ] Development tools + LSP support

> Specifically adding autocompletion for Dawn APIs.

- [x] Run/Stop State Support

## Building from Source

If you wish to build from source, we recommend doing so in the Nix Flake provided, as all of the dependencies
this project uses are inside of the flake. If your OS doesn't support Nix (or you're on Windows), we recommend
using your package manager to download dependencies. You can see the list of dependencies in the Nix Flake.

Once all dependencies are downloaded, run

```sh
cargo build --release
```

and a production-ready build will be available in `./target/release/`
