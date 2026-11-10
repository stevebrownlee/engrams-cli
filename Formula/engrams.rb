class Engrams < Formula
  desc "Standalone, high-performance Rust CLI for managing contextual memory"
  homepage "https://github.com/stevebrownlee/engrams-cli"
  version "0.17.0"

  if OS.mac?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-apple-darwin.tar.gz"
      sha256 "039ea90551ba4f5d0ba7e59e0aba96e3f6cc5c6829ba73523f246f1d8414ecb6"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-apple-darwin.tar.gz"
      sha256 "c09e08011393c84547903cec3245811c425a6880bd301c06cdbe8da7ce21fc8b"
    end
  elsif OS.linux?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-unknown-linux-musl.tar.gz"
      sha256 "e5e7fe84d1f2bee9d702d82d485e03a04f5b5ac1cda2b976ed647ae46bf608a0"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-unknown-linux-musl.tar.gz"
      sha256 "1678b8e3815f713cdbabe9b41b56c2a7c2324b227b9b73790c2a6fb5c61b74bc"
    end
  end

  def install
    bin.install "engrams"
  end

  test do
    system "#{bin}/engrams", "--version"
  end
end
