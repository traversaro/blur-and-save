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
- To release, bump the version in `Cargo.toml`, update `Cargo.lock`, and push to `main`. The Windows workflow builds and tests the executable with Pixi, including an offline smoke test. The release job signs and verifies the executable before publishing `v<version>`. Use **Run workflow** on `main` or rerun the failed release job after completing signing setup.
- The smoke test argument `--offline-smoke-test INPUT OUTPUT` is for CI. It passes a known photo through the same detector and export code as the GUI.

## Windows release signing

- Enroll a publicly trusted [SSL.com code-signing certificate](https://www.ssl.com/products/software-integrity/code-signing/) in [eSigner](https://www.ssl.com/how-to/automate-esigner-ev-code-signing/). Enable automation for the credential that signs Blur & Save.
- In GitHub repository settings, configure the `windows-signing` environment for `main`. Add the environment secrets `ESIGNER_USERNAME`, `ESIGNER_PASSWORD`, `ESIGNER_CREDENTIAL_ID`, and `ESIGNER_TOTP_SECRET`. Add `WINDOWS_SIGNER_THUMBPRINT` as an environment variable containing the certificate's 40-character Windows thumbprint. Update it when the signing certificate changes.
- Use the Windows release job's signature check when troubleshooting. It requires Windows to validate the embedded Authenticode signature, find a timestamp, and match the configured certificate. Publish the exact executable that passed those checks.

## Microsoft Store package

- Reserve the app as an **MSIX or PWA app** in [Microsoft Partner Center](https://partner.microsoft.com/dashboard). Under **Product management → Product identity**, copy **Package/Identity/Name**, **Package/Identity/Publisher**, and **Package/Properties/PublisherDisplayName**. Use the exact reserved name for the display name.
- Run **Microsoft Store MSIX** from **Actions** with those four values. Download the `blur-and-save-store` artifact and upload the **.msix** inside the ZIP under **Packages** in Partner Center. Microsoft signs the package after certification.
- Packaging changes on `main` also produce a `blur-and-save-packaging-test` artifact using a test identity. Use it to verify the packaging pipeline. Partner Center requires the identity from your reserved app for uploads.
- The MSIX version is `(Cargo major + 1).minor.patch.0` because the Store requires a nonzero first component and reserves the fourth. App 0.0.2 packages as 1.0.2.0.
- [Store listing copy](store/listing.md) includes the release's privacy policy, support link, restricted capability justification, and test instructions. Complete the age rating questionnaire in Partner Center before submitting.
