class Engrams < Formula
  desc "Standalone, high-performance Rust CLI for managing contextual memory"
  homepage "https://github.com/stevebrownlee/engrams-cli"
  version "0.10.0"

  if OS.mac?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-apple-darwin.tar.gz"
      sha256 "1659a7f7ed32d2666394849330589e33714e1267e572408d23ccd5656dca535c"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-apple-darwin.tar.gz"
      sha256 "380c104e3d07f20811f778f8f54304c5da7b935abddf0841877d40c19fb6d9e1"
    end
  elsif OS.linux?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-unknown-linux-musl.tar.gz"
      sha256 "9d43e2491d2343d60ec27394469f960c221e3218545637fc470c47ede29fb9a6"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-unknown-linux-musl.tar.gz"
      sha256 "1bb45a284797c1f700ec301af2fbf6728e74088cda2eda50847b6090d63cdfc5"
    end
  end

  def install
    bin.install "engrams"
  end

  test do
    system "#{bin}/engrams", "--version"
  end
end
