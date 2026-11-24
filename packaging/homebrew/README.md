# Homebrew cask

`locite.rb` is the draft cask for [homebrew/cask](https://github.com/Homebrew/homebrew-cask). Homebrew and GitHub Releases are the only distribution channels; the Mac App Store's terms conflict with the GPL, so Locite is not offered there.

## After a release is published

1. Take the DMG's hash from the release's `SHA256SUMS` (verify it first, see [docs/building.md](../../docs/building.md)):

   ```sh
   gh release download vX.Y.Z --repo skyoo2003/locite --pattern SHA256SUMS
   grep '_aarch64.dmg$' SHA256SUMS
   ```

2. Set `version "X.Y.Z"` and `sha256 "<that hash>"` in `locite.rb`.
3. Check the cask from a local tap:

   ```sh
   brew tap-new --no-git "$USER/local" && brew trust "$USER/local"
   cp packaging/homebrew/locite.rb "$(brew --repo "$USER/local")/Casks/"
   brew style --cask "$USER/local/locite"
   brew audit --new --cask "$USER/local/locite"
   brew install --cask "$USER/local/locite" && brew uninstall --zap --cask "$USER/local/locite"
   brew untap "$USER/local"
   ```

4. Submit it: fork Homebrew/homebrew-cask, add the file as `Casks/l/locite.rb`, and open a pull request following their [contributing guide](https://github.com/Homebrew/homebrew-cask/blob/main/CONTRIBUTING.md). Later versions go through `brew bump-cask-pr --version X.Y.Z locite`.

`brew audit --new` also requires the GitHub repository to be notable (at least 30 forks, 30 watchers, or 75 stars) and the homepage to be reachable, so the first submission waits for both.
