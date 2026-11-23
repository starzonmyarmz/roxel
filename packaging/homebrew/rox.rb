class Rox < Formula
  desc "Command-line companion to the Roxel voxel editor"
  homepage "https://github.com/starzonmyarmz/roxel"
  license "MIT"

  on_macos do
    url "https://github.com/starzonmyarmz/roxel/releases/download/v@VERSION@/rox-macos.zip"
    sha256 "@MACOS_SHA256@"
  end

  on_linux do
    on_intel do
      url "https://github.com/starzonmyarmz/roxel/releases/download/v@VERSION@/rox-linux.tar.gz"
      sha256 "@LINUX_SHA256@"
    end
  end

  def install
    bin.install "rox"
  end

  test do
    assert_equal "rox #{version}", shell_output("#{bin}/rox --version").strip
  end
end
