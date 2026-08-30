class Engrams < Formula
  desc "Standalone, high-performance Rust CLI for managing contextual memory"
  homepage "https://github.com/stevebrownlee/engrams-cli"
  version "0.12.1"

  if OS.mac?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-apple-darwin.tar.gz"
      sha256 "2db71bdae2aa7e71a2a93290c521266af6e29026535241b9136316b706a4999f"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-apple-darwin.tar.gz"
      sha256 "2c2d49f99ad8d1f9f235cf98549c7dfc2268b6962fa6f3e25995fe870489de1b"
    end
  elsif OS.linux?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-unknown-linux-musl.tar.gz"
      sha256 "8f604e6522a8c837c514c46d1362806388a8e1c4d14d4cb82f8fa2209a95be53"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-unknown-linux-musl.tar.gz"
      sha256 "1a26a8caa4fe9c025f9cff2b2532ad6d193682291e41bcf231d7f9f0814729bb"
    end
  end

  def install
    bin.install "engrams"
  end

  test do
    system "#{bin}/engrams", "--version"
  end
end
