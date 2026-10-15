# Packaging and releases

## Making a release

1. Update `version` in `Cargo.toml` and add a `<release>` entry to
   `io.github.arczimboldo.Hepcat.metainfo.xml`.
2. If `Cargo.lock` changed since the last release, regenerate the Flatpak
   crate list (needs Python with `aiohttp` and `tomlkit`):
   ```sh
   curl -LO https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
   python3 flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
   ```
3. Commit, then tag and push: `git tag v0.1.0 && git push origin main v0.1.0`.
   The Flatpak workflow builds `hepcat.flatpak` and attaches it to the
   GitHub release for that tag (create the release from the tag on GitHub
   if it does not exist yet).

## Arch Linux (`arch/`)

- `hepcat` builds a tagged release. After tagging, set `pkgver`, run
  `updpkgsums` (from `pacman-contrib`) to fill in the checksum, and
  `makepkg --printsrcinfo > .SRCINFO`.
- `hepcat-git` builds the latest commit and needs no updates per release.
- Both disable LTO: makepkg's GCC LTO objects from the C++ parts cannot be
  linked by Rust's `lld`.

To publish on the AUR once you have an account: for each package,
`git clone ssh://aur@aur.archlinux.org/<pkgname>.git`, copy in `PKGBUILD`,
`hepcat.install` and `.SRCINFO`, commit and push.

## Flatpak (`flatpak/`)

Build and install locally from the repository root:

```sh
flatpak install --user flathub org.flatpak.Builder org.kde.Sdk//6.11 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08 org.freedesktop.Sdk.Extension.llvm21//25.08
flatpak run org.flatpak.Builder --user --install --force-clean \
  build-dir packaging/flatpak/io.github.arczimboldo.Hepcat.yml
```

Notes on the sandbox:

- Access to PipeWire is `--filesystem=xdg-run/pipewire-0`. Hepcat connects
  with `media.category = Manager`, which WirePlumber needs before it lets a
  Flatpak move streams or set default devices.
- "Start at Login" goes through the Background portal.
- The single-instance socket lives in `$XDG_RUNTIME_DIR/app/$FLATPAK_ID`.
- Config is in `~/.var/app/io.github.arczimboldo.Hepcat/config/hepcat`.

### Submitting to Flathub

Follow <https://docs.flathub.org/docs/for-app-authors/submission>. In short:

1. Add screenshots to the metainfo (`<screenshots>` with image URLs from the
   repository, e.g. `https://raw.githubusercontent.com/arczimboldo/hepcat/v0.1.0/docs/screenshot.png`).
   Flathub requires at least one.
2. In a copy of the manifest, replace the `dir` source with the release:
   ```yaml
   - type: git
     url: https://github.com/arczimboldo/hepcat.git
     tag: v0.1.0
     commit: <full commit hash of the tag>
   ```
   and put `cargo-sources.json` next to it.
3. Fork `flathub/flathub`, branch off `new-pr`, add the two files, and open a
   pull request against `new-pr`. Reviewers will comment there; the
   `xdg-run/pipewire-0` permission is expected for an audio manager (Helvum,
   qpwgraph and EasyEffects use it), so mention what Hepcat does in the PR.
