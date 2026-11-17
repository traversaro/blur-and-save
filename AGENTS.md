# Blur & Save development

- Keep the README focused on downloading and using the app. Put build, test, and release instructions here.
- Use Pixi for the Rust toolchain. Run `pixi install --locked`, then `pixi run test`, `pixi run build`, or `pixi run run`.
- Run `pixi run cargo fmt --check` and `pixi run cargo clippy --locked --all-targets -- -D warnings` before publishing.
- Windows builds use the MSVC x64 toolchain and Windows SDK from Visual Studio Build Tools alongside Pixi. The release build statically links the MSVC runtime and uses Direct3D 12 through egui/eframe. Linux works as a development platform with OpenGL and a desktop file chooser.
- `src/main.rs` owns the egui interface and worker jobs. `src/detect.rs` runs the bundled UltraFace model on CPU with tract. `src/photo.rs` handles orientation, region edits, and PNG export.
- Keep all inference offline. The executable embeds the model and icon and needs no DLL sidecars, Python, installer, or model download. Do not add background network requests, telemetry, or an update check.
- The detector produces normalized face boxes. Keep blur and black-box rendering clipped to those boxes or to regions the user drew. Preserve pixels outside selected regions. Export a new full-resolution PNG without input metadata. Never overwrite source or existing files.
- The bundled [UltraFace RFB-640 model](https://github.com/Linzaer/Ultra-Light-Fast-Generic-Face-Detector-1MB/blob/dffdddda9794a50607cba8f318507a28c1c27cab/models/onnx/version-RFB-640.onnx) is MIT-licensed. Its SHA-256 is `8f4c659275977e7a3bfbfa339a9c769ad793df50f9c0baa8c14b11baa1646430`. Keep its license in `assets/ULTRAFACE-LICENSE.txt`.
- The detection test uses a resized copy of [NASA's public-domain Eileen Collins photo](https://scikit-image.org/docs/0.25.x/api/skimage.data.html#skimage.data.astronaut), distributed as the scikit-image astronaut fixture. Keep the fixture in tests only.
- To release, bump the version in `Cargo.toml`, update `Cargo.lock`, and push to `main`. The Windows workflow runs tests, builds the locked executable using Pixi, blocks it from making outbound connections during a smoke test, and creates the matching `v<version>` release if it does not exist. It can also be run manually from `main`.
- The smoke test argument `--offline-smoke-test INPUT OUTPUT` is for CI. It passes a known photo through the same detector and export code as the GUI.
