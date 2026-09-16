# Releasing Winlane

Winlane ships a separate macOS app for Apple Silicon (`arm64`) and Intel (`x86_64`). Both require macOS 14 or later. Each architecture has a ZIP containing `Winlane.app` and a DMG with an Applications shortcut for drag-and-drop installation. There is no installer service or automatic updater.

## Continuous integration

The **CI** workflow runs on pull requests and pushes to `master`, and can be started manually from Actions. It checks formatting, Clippy, and tests on native Apple Silicon and Intel macOS 15 runners, plus ShellCheck and actionlint on Linux. It also builds release bundles with ad hoc signatures, packages them, verifies the DMG, and checks the extracted ZIP's executable and signature. Native panel tests use hidden windows; if the runner has no display, those checks report a skip.

Download test builds from the workflow's `Winlane-ci-arm64` and `Winlane-ci-x86_64` artifacts. They expire after seven days. These are not notarized and should not replace a locally signed development installation. CI does not request Accessibility permission or test interactive window activation.

GitHub Actions are pinned to commit SHAs. Dependabot checks Actions and Cargo dependencies weekly. The workflows currently use Rust 1.95.0; update both workflow files together when changing the release toolchain.

## Publish a version

1. Update the package version in `Cargo.toml`, refresh `Cargo.lock`, and commit the tested changes to `master`.
2. Push a matching tag, such as `v0.8.3` for package version `0.8.3`:

   ```sh
   git tag -a v0.8.3 -m 'Winlane 0.8.3'
   git push origin v0.8.3
   ```

3. The **Release** workflow runs the same CI checks, verifies the tag matches the package version and is on `master`, then builds both targets from the tagged source. It uploads all assets into a draft and publishes the release after the upload succeeds.

The resulting assets are:

```text
Winlane-0.8.3-macos-arm64.dmg
Winlane-0.8.3-macos-arm64.zip
Winlane-0.8.3-macos-x86_64.dmg
Winlane-0.8.3-macos-x86_64.zip
SHA256SUMS
build-info.json
```

`build-info.json` records the commit, compiler, signing mode, workflow run, and package hashes. Download all assets into one directory and run `shasum -a 256 -c SHA256SUMS` to verify them. A checksum detects download corruption; verify that it comes from the intended release.

A release with an existing tag is never overwritten. If an upload fails and leaves an incomplete draft, inspect and remove that draft before rerunning the workflow. A published version should be fixed with a new version and tag. Repository visibility also controls access to releases and Actions artifacts: private repositories do not provide public download links.

## Signing and notarization

Without distribution credentials, releases use **ad hoc signing**. The release notes state that they are not Apple-notarized, explain macOS's first-launch approval, and note that upgrades can require Accessibility authorization again. No developer certificate is needed for this mode.

For notarized releases, configure all six repository Actions secrets:

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12` | Base64-encoded `.p12` containing a **Developer ID Application** certificate and its private key. |
| `MACOS_CERTIFICATE_PASSWORD` | Password protecting the `.p12`. |
| `MACOS_SIGNING_IDENTITY` | Exact Developer ID Application certificate name or SHA-1 fingerprint. |
| `MACOS_NOTARY_KEY` | Base64-encoded App Store Connect team API `.p8` key. |
| `MACOS_NOTARY_KEY_ID` | API key ID. |
| `MACOS_NOTARY_ISSUER` | API issuer ID. |

Encode files with `base64 -i certificate.p12 | tr -d '\n'` and the equivalent command for the `.p8` file. Keep the source files, passwords, and encoded values outside the repository. Use a team API key authorized for Apple's notary service; individual keys do not use the issuer-based configuration above.

When all six secrets are present, the workflow imports them into a temporary keychain on the release runner, signs with hardened runtime and a timestamp, submits each app and DMG to Apple, and staples the accepted tickets. The ZIP is created after stapling its app. The temporary keychain and credential files are removed in a final cleanup step. An incomplete configuration or a signing/notarization failure stops the release; it does not fall back to ad hoc signing.

Once notarized releases are enabled, also set the repository Actions variable **`WINLANE_REQUIRE_NOTARIZATION=true`**. This prevents an accidentally removed set of secrets from producing an ad hoc release.

The local **Windowlane Development** certificate is for development only. Do not upload it as a distribution credential. The bundle identifier remains `app.windowlane.desktop`, so local development keeps its existing identity. Moving between development and Developer ID signed versions can require one permission migration.

Apple's [notarization guide](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) describes the certificate and account requirements. This repository does not create certificates or configure GitHub secrets automatically.

## Package locally

On macOS with Rust and Xcode Command Line Tools:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/build-app.sh --adhoc --target aarch64-apple-darwin --output-dir dist/arm64
./scripts/package-app.sh dist/arm64/Winlane.app dist/packages
```

Use `x86_64-apple-darwin` for an Intel build. Packaging refuses to overwrite existing ZIP/DMG files; use a fresh output directory for a repeat build. The commands above neither install nor launch the app.

For a configured local distribution keychain, use `--distribution` instead of `--adhoc` and set `WINLANE_SIGNING_IDENTITY` and `WINLANE_SIGNING_KEYCHAIN`. Set `WINLANE_NOTARY_PROFILE` to a `notarytool` credential profile in that same keychain when calling `package-app.sh` to notarize and staple the packages. Local developer signing remains the default when neither signing flag is supplied.
