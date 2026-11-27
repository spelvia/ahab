# Releasing Ahab

1. Open a PR that sets the version in `crates/ahab/Cargo.toml`,
   `apps/native/Cargo.toml` and `apps/ui/package.json`, updates the lockfiles
   and `crates/ahab/openapi.json` (`make openapi`), and adds
   `docs/releases/<version>.md`: what changed for people using Ahab, and how
   to adapt. Use versions such as `0.1.0-rc.1` and `0.1.0`.
2. Merge it and wait for the push build on main to pass. Tag that commit:

   ```sh
   git fetch origin main
   git tag -a v0.1.0 <tested-main-sha> -m 'Ahab 0.1.0'
   git push origin refs/tags/v0.1.0
   ```

3. The Release workflow checks the tag, the notes and main's build of the
   commit, drafts the GitHub Release with the notes and `scripts/install.sh`,
   builds `ahab` for Linux x86_64 and arm64 (on Debian 12, so it runs on
   glibc 2.36 and newer) and macOS arm64, uploads each
   `ahab-<os>-<arch>.tar.gz` with its `.sha256`, and publishes the release.
   Until 0.1.0, release candidates are full releases too, marked "latest",
   so the install script's default (`releases/latest`) always gets the
   newest one. From 0.1.0 on, mark them pre-releases again.
4. Install it from the release on a clean machine (`README.md`,
   Installation) and check `ahab --version`.

To try the builds before tagging, run the Release workflow by hand on a
branch (`gh workflow run release.yml --ref main`): it builds every platform
and publishes nothing.

If a build fails, fix it on main and release a new version: a published
version is never rebuilt. A failed run leaves a draft release, which can be
deleted along with its tag before tagging again.
