# Draft cask for homebrew/cask. See README.md next to this file for how a release updates it.
cask "locite" do
  version "0.1.0"
  sha256 "0000000000000000000000000000000000000000000000000000000000000000"

  url "https://github.com/skyoo2003/locite/releases/download/v#{version}/Locite_#{version}_aarch64.dmg"
  name "Locite"
  desc "Search and summarize local documents on-device with cited sources"
  homepage "https://skyoo2003.github.io/locite"

  depends_on arch: :arm64
  depends_on :macos

  app "Locite.app"

  # The database key stays in the login keychain (service io.github.skyoo2003.locite); remove it there by hand.
  zap trash: [
    "~/Library/Application Support/io.github.skyoo2003.locite",
    "~/Library/Caches/io.github.skyoo2003.locite",
    "~/Library/Logs/io.github.skyoo2003.locite",
    "~/Library/WebKit/io.github.skyoo2003.locite",
  ]
end
