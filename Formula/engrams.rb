class Engrams < Formula
  desc "Standalone, high-performance Rust CLI for managing contextual memory"
  homepage "https://github.com/stevebrownlee/engrams-cli"
  version "0.11.0"

  if OS.mac?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-apple-darwin.tar.gz"
      sha256 "a1111f55225ed104bc11708ca01bf1b64ffa0ad36702c8732972f404e870b209"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-apple-darwin.tar.gz"
      sha256 "b8bc2b2ff063cee24d9b2c3c550642f40793d784b50c8e7382eac389b5215579"
    end
  elsif OS.linux?
    if Hardware::CPU.intel?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-x86_64-unknown-linux-musl.tar.gz"
      sha256 "0751a4de7377e61042107d08c9646c4672e81e8a87ce1768e504bda0f321614b"
    elsif Hardware::CPU.arm?
      url "https://github.com/stevebrownlee/engrams-cli/releases/download/v#{version}/engrams-aarch64-unknown-linux-musl.tar.gz"
      sha256 "5943ced79d3703e5af9fa98bc16b80ce65f2996a2b387543b4e43324a11a6a88"
    end
  end

  def install
    bin.install "engrams"
  end

  test do
    system "#{bin}/engrams", "--version"
  end
end
