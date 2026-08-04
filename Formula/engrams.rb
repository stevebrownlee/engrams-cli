class Engrams < Formula
  desc "Standalone, high-performance Rust CLI for managing contextual memory"
  homepage "https://github.com/stevebrownlee/engrams-cli"
  version "0.8.1"

  if OS.mac?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-apple-darwin.tar.gz"
      sha256 "737a229c17a264d7b2c2db9333f6cd0359f6e40f91c6f34efcc9f84065000f84"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-apple-darwin.tar.gz"
      sha256 "607ba01c33b5d73ddc833cfd99bbafd8d52564573c75724207d2486b9039b241"
    end
  elsif OS.linux?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-unknown-linux-musl.tar.gz"
      sha256 "236151795159996ec0e8348e67aa78ab99a44da7a4e5842272155e69b3a3db9a"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-unknown-linux-musl.tar.gz"
      sha256 "5bd895561770248d41c9a298e0823d0dcaa5384de7f4cad33ce8c2e6e4fbb14a"
    end
  end

  def install
    bin.install "engrams"
  end

  test do
    system "#{bin}/engrams", "--version"
  end
end
