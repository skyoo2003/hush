# Template for Casks/locite.rb in skyoo2003/homebrew-tap. scripts/bump-homebrew-tap.sh fills in version
# and sha256 from a published release; see README.md next to this file.
cask "locite" do
  version "0.0.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/skyoo2003/locite/releases/download/v#{version}/Locite_#{version}_aarch64.dmg"
  name "Locite"
  desc "Search and summarize local documents on-device with cited sources"
  homepage "https://github.com/skyoo2003/locite"

  livecheck do
    skip "Updated by scripts/bump-homebrew-tap.sh in the source repository."
  end

  depends_on arch: :arm64
  depends_on :macos

  app "Locite.app"

  # Releases are signed with the project's own certificate, not notarized (ADR-021), so Gatekeeper
  # would block the first launch.
  # The sha256 above already pins the exact DMG of the release.
  postflight_steps do
    run "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "{{appdir}}/Locite.app"]
  end

  # The database key stays in the login keychain (service io.github.skyoo2003.locite); remove it there by hand.
  zap trash: [
    "~/Library/Application Support/io.github.skyoo2003.locite",
    "~/Library/Caches/io.github.skyoo2003.locite",
    "~/Library/Logs/io.github.skyoo2003.locite",
    "~/Library/WebKit/io.github.skyoo2003.locite",
  ]
end
