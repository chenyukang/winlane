# Releasing Winlane

Winlane ships a separate macOS app for Apple Silicon (`arm64`) and Intel (`x86_64`). Both require macOS 14 or later. Each architecture has a ZIP containing `Winlane.app` and a DMG with an Applications shortcut for drag-and-drop installation. Sparkle 2 provides in-app updates using the ZIP archives.

## Continuous integration

The **CI** workflow runs on pull requests and pushes to `master`, and can be started manually from Actions. It checks formatting, Clippy, and tests on native Apple Silicon and Intel macOS 15 runners, plus ShellCheck and actionlint on Linux. It also builds release bundles with ad hoc signatures, tests Sparkle's runtime bridge and an isolated update/install/relaunch cycle, verifies rejection of tampered feeds and archives, packages the app, verifies the DMG, and checks the extracted ZIP's executable and signature. Native panel tests use hidden windows; if the runner has no display, those checks report a skip.

Download test builds from the workflow's `Winlane-ci-arm64` and `Winlane-ci-x86_64` artifacts. They expire after seven days. These are not notarized and should not replace a locally signed development installation. CI does not request Accessibility permission or test interactive window activation.

GitHub Actions are pinned to commit SHAs. Dependabot checks Actions and Cargo dependencies weekly. The workflows currently use Rust 1.95.0; update both workflow files together when changing the release toolchain.

## Publish a version

1. Update the package version in `Cargo.toml`, refresh `Cargo.lock`, and commit the tested changes to `master`.
2. Push a matching tag, such as `v0.8.6` for package version `0.8.6`:

   ```sh
   git tag -a v0.8.6 -m 'Winlane 0.8.6'
   git push origin v0.8.6
   ```

3. The **Release** workflow runs the same CI checks, verifies the tag matches the package version and is on `master`, then builds both targets from the tagged source. It uploads all assets into a draft and publishes the release after the upload succeeds.

The resulting assets are:

```text
Winlane-0.8.6-macos-arm64.dmg
Winlane-0.8.6-macos-arm64.zip
Winlane-0.8.6-macos-x86_64.dmg
Winlane-0.8.6-macos-x86_64.zip
appcast-arm64.xml
appcast-x86_64.xml
SHA256SUMS
build-info.json
```

`build-info.json` records the commit, compiler, signing mode, workflow run, and package hashes. Download all assets into one directory and run `shasum -a 256 -c SHA256SUMS` to verify them. A checksum detects download corruption; verify that it comes from the intended release.

A release with an existing tag is never overwritten. If an upload fails and leaves an incomplete draft, inspect and remove that draft before rerunning the workflow. A published version should be fixed with a new version and tag. Repository visibility also controls access to releases and Actions artifacts: private repositories do not provide public download links.

To verify signing credentials without publishing a version, run the **Release** workflow manually on `master`. It runs the checks and packages both architectures, then uploads `Winlane-signing-verification` as a three-day Actions artifact. Only a tag push publishes a GitHub release.

## Sparkle updates

Release builds embed the pinned Sparkle version from `scripts/fetch-sparkle.sh`. Downloads are verified against its recorded SHA-256 before extraction. Sparkle's unused sandbox XPC services are omitted; its installer and relaunch helpers are signed inside out with the app's identity.

Each architecture uses its own `releases/latest/download/appcast-ARCH.xml` feed. Feed entries point to immutable, versioned GitHub Release ZIP URLs, not to `latest` archives. `generate-appcast.sh` signs both the ZIP entry and XML feed with Sparkle's official tools. It verifies the feed signature, checks the ZIP against the public key embedded in Winlane, and checks version, URL, and minimum OS before the workflow publishes all assets together. The release fails if the key or either feed is missing. The first integration uses full ZIP updates without deltas.

Generate the update key once, separately from the app's code-signing certificate:

```sh
sparkle_dir=$(./scripts/fetch-sparkle.sh)
"$sparkle_dir/bin/generate_keys" --account app.windowlane.desktop
```

Keep its public key in `resources/sparkle-public-key.txt`. Export the private key with `generate_keys --account app.windowlane.desktop -x /secure/temporary/key` and set the repository Actions secret **`SPARKLE_ED_KEY`** to the exported file's contents (already base64-encoded). Remove the temporary export after uploading it. The release job passes the secret on standard input, never as a command-line argument. Local appcast generation uses the named Keychain entry when the environment variable is unset.

Keep a secure backup of this key outside Git. Do not regenerate it for each release. Without Developer ID signing, losing it requires a manual app installation to establish a new update identity. The update key and the persistent app certificate have different roles: Ed25519 verifies feed/package authenticity; the app certificate preserves the macOS application identity. Sparkle does not replace Apple notarization.

Daily checks are enabled by default; silent downloading and installation are disabled. Both feed signatures and archive verification before extraction are required. The user's automatic-check choice is persisted by Sparkle in the app's preferences (`SUEnableAutomaticChecks`), outside Winlane's configuration JSON. Ordinary local builds omit Sparkle; `--with-updater` enables it explicitly.

Users of versions without Sparkle must manually install the first updater-enabled release. Later releases can be installed through **Check for Updates…**. Keep the bundle identifier, certificate, update public key, and installation location stable.

## Signing and notarization

The release workflow supports three modes: a persistent certificate without notarization, Developer ID signing with notarization, and ad hoc signing for repositories without credentials. Partial credentials always fail the release instead of falling back to another mode.

### Persistent signing without an Apple account

Create one self-signed **Code Signing** identity in Keychain Access, as described in [the development guide](development.md#development-signing). Keep reusing that certificate and private key. Export the identity as a password-protected `.p12`, then configure these repository Actions secrets:

| Secret | Value |
| --- | --- |
| `MACOS_CERTIFICATE_P12` | Base64-encoded `.p12` containing the certificate and its private key. |
| `MACOS_CERTIFICATE_PASSWORD` | Password protecting the `.p12`. |
| `MACOS_SIGNING_IDENTITY` | Certificate SHA-1 fingerprint (recommended) or exact certificate name. |

Set the repository Actions variable **`WINLANE_REQUIRE_SIGNING=true`** so removing all signing secrets cannot silently produce an ad hoc release. Leave the `MACOS_NOTARY_*` secrets unset for this mode. The workflow imports the identity into a temporary keychain, signs both app bundles, and verifies the packaged signatures without contacting Apple's notary service. `build-info.json` records this mode as `certificate`.

Keep an encrypted backup of the identity and its export password outside Git. Creating a new certificate with the same name does not preserve the old signing identity. You can reuse the existing local `Windowlane Development` identity when local and CI builds should share Accessibility authorization; its private key will then also be available to this repository's release job.

These builds are not Apple-notarized and can still require first-launch approval in **System Settings → Privacy & Security → Open Anyway**. Switching from ad hoc builds or another certificate may require granting Accessibility permission once more. Subsequent updates signed with the same identity should retain that permission; keep the bundle identifier and installation location unchanged.

### Developer ID signing and Apple notarization

Use a **Developer ID Application** identity for the three certificate secrets above, and also configure:

| Secret | Value |
| --- | --- |
| `MACOS_NOTARY_KEY` | Base64-encoded App Store Connect team API `.p8` key. |
| `MACOS_NOTARY_KEY_ID` | API key ID. |
| `MACOS_NOTARY_ISSUER` | API issuer ID. |

Encode files with `base64 -i certificate.p12 | tr -d '\n'` and the equivalent command for the `.p8` file. Keep the source files, passwords, and encoded values outside the repository. Use a team API key authorized for Apple's notary service; individual keys do not use the issuer-based configuration above.

When all six secrets are present, the workflow imports them into a temporary keychain on the release runner, signs with hardened runtime and a timestamp, submits each app and DMG to Apple, and staples the accepted tickets. The ZIP is created after stapling its app. The temporary keychain and credential files are removed in a final cleanup step. An incomplete configuration or a signing/notarization failure stops the release; it does not fall back to ad hoc signing.

Once notarized releases are enabled, also set the repository Actions variable **`WINLANE_REQUIRE_NOTARIZATION=true`**. This prevents missing notary credentials from producing an unnotarized release.

A self-signed **Windowlane Development** certificate cannot be used for Apple notarization. The bundle identifier remains `app.windowlane.desktop`. Moving between self-signed and Developer ID signed versions can require one permission migration.

Apple's [notarization guide](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) describes the certificate and account requirements. This repository does not create certificates or configure GitHub secrets automatically.

### Ad hoc builds

When no macOS certificate credentials or signing requirements are configured, releases use **ad hoc signing**. The Sparkle update-signing key is still required. Ad hoc app signatures change with the executable, so upgrades can require Accessibility authorization again. Pull-request CI always uses ad hoc signatures and ephemeral test update keys; it never receives the release signing credentials.

## Package locally

On macOS with Rust and Xcode Command Line Tools:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
./scripts/build-app.sh --adhoc --target aarch64-apple-darwin --output-dir dist/arm64
./scripts/package-app.sh dist/arm64/Winlane.app dist/packages
```

Use `x86_64-apple-darwin` for an Intel build. Packaging refuses to overwrite existing ZIP/DMG files; use a fresh output directory for a repeat build. The commands above neither install nor launch the app.

For a persistent self-signed build, omit `--adhoc` and set `WINLANE_SIGNING_IDENTITY` and, if needed, `WINLANE_SIGNING_KEYCHAIN`. Local developer signing is the default when neither signing flag is supplied.

For a configured Developer ID keychain, use `--distribution` instead of `--adhoc` and set `WINLANE_SIGNING_IDENTITY` and `WINLANE_SIGNING_KEYCHAIN`. Set `WINLANE_NOTARY_PROFILE` to a `notarytool` credential profile in that same keychain when calling `package-app.sh` to notarize and staple the packages.
