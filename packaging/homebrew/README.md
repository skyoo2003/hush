# Homebrew cask

`locite.rb` is the template for `Casks/locite.rb` in the project tap, [skyoo2003/homebrew-tap](https://github.com/skyoo2003/homebrew-tap):

```sh
brew install --cask skyoo2003/tap/locite
```

Homebrew and GitHub Releases are the only distribution channels. The Mac App Store's terms conflict with the GPL, so Locite is not offered there. Releases are signed with the project's own certificate and not notarized ([ADR-021](../../docs/decisions.md)), and homebrew/cask does not take casks that fail Gatekeeper, so the cask lives in the project tap. Its `postflight_steps` clear the quarantine flag on the installed app, and its `sha256` pins the release DMG.

## After a release is published

```sh
DRY_RUN=1 bash scripts/bump-homebrew-tap.sh X.Y.Z   # prints the cask
bash scripts/bump-homebrew-tap.sh X.Y.Z             # opens a pull request in the tap
```

The script downloads the DMG and `SHA256SUMS`, checks both against their build provenance attestations and the DMG against `SHA256SUMS`, and fills `version` and `sha256` into this template. Merge the pull request in the tap after checking it from a local tap:

```sh
brew tap-new --no-git "$USER/local" && brew trust "$USER/local"
cp locite.rb "$(brew --repo "$USER/local")/Casks/"   # the file from the pull request
brew style --cask "$USER/local/locite" && brew audit --cask --strict "$USER/local/locite"
brew install --cask "$USER/local/locite" && brew uninstall --zap --cask "$USER/local/locite"
brew untap "$USER/local"
```

`brew style` only checks a cask inside a tap; on a loose file it applies the plain Ruby rules. The first time, also add a `locite` row to the tap's README table.
