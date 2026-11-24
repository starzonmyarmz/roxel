cask "roxel" do
  version "@VERSION@"
  sha256 "@SHA256:Roxel-macos.zip@"

  url "https://github.com/starzonmyarmz/roxel/releases/download/v#{version}/Roxel-macos.zip"
  name "Roxel"
  desc "Voxel editor"
  homepage "https://github.com/starzonmyarmz/roxel"

  app "Roxel.app"

  zap trash: "~/Library/Application Support/roxel"

  caveats <<~EOS
    Roxel is self-signed and not notarized, so macOS blocks its first launch.
    Right-click Roxel.app in Applications and choose Open, or run:
      xattr -dr com.apple.quarantine #{appdir}/Roxel.app
  EOS
end
