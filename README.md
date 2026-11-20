# Visual Studio Code for Tezbar (Rust testing preview)

Standalone Rust SDK process with seven ordinary commands: recent projects, selected file/folder launch, new window, installed extensions, Marketplace install, Command Palette, and Open Agents Window. Requires Tezbar 0.3.0-beta.12 or later. No JavaScript runtime is used.

Original MIT source: [Raycast extension](https://github.com/raycast/extensions/tree/cd75ae8af9cf9ed5bc18a26a4fd0d81d5ed217e3/extensions/visual-studio-code-recent-projects). Agents Window refresh: pinned commit `886cbd73fe377ad1d8f0c17083e6cd4c96748a33`, with exact reference blobs and hashes under `reference/agents-window-886cbd73/`.

`cargo test --locked --all-targets` runs owned fixtures. Build locally with `cargo build --locked --release --bin tezbar-visual-studio-code --example fixture`, then run `python scripts/process-check.py target/release/examples/fixture target/release/tezbar-visual-studio-code` (add `.exe` on Windows). Tests record host calls and never open real editors or touch the real clipboard. Package locally with `scripts/package-beta9.py`; it supports macOS arm64 and Windows x64.

This is a testing preview with incomplete native rendering and integration parity. See PORT-REPORT.md for scope, retained preferences, legacy pin migration and remaining gaps. Store command coverage does not establish desktop acceptance. No real-editor acceptance has been completed. Newer upstream changes beyond the Agents Window command are not included automatically.
