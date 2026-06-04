{
  version = "0.6.0-rc.0";

  # SHA256 SRI hashes of each prebuilt artifact published in the matching
  # GitHub Release. This pins the prebuilt package to whatever release line the
  # default branch currently carries: a -dev.N during normal development, or the
  # active -rc.N while a release stabilizes (main tracks the release line during
  # the RC window — see docs/RELEASE.md), and finally the stable vX.Y.Z.
  #
  # To refresh after a new release:
  #
  #   ver=X.Y.Z[-dev.N]
  #   for arch in amd64 arm64; do
  #     curl -fsSL -o /tmp/dbflux-$arch.tar.gz \
  #       "https://github.com/0xErwin1/dbflux/releases/download/v$ver/dbflux-linux-$arch.tar.gz"
  #     nix-hash --to-sri --type sha256 \
  #       "$(sha256sum /tmp/dbflux-$arch.tar.gz | cut -d' ' -f1)"
  #   done
  #
  # Then update `version`, the two `url`s, and the two `hash`es below.
  artifacts = {
    "x86_64-linux" = {
      url = "https://github.com/0xErwin1/dbflux/releases/download/v0.6.0-rc.0/dbflux-linux-amd64.tar.gz";
      hash = "sha256-LN42s/N0pTRea3oN5qzA9TfihW41H0mb0nQkFdUMGck=";
    };
    "aarch64-linux" = {
      url = "https://github.com/0xErwin1/dbflux/releases/download/v0.6.0-rc.0/dbflux-linux-arm64.tar.gz";
      hash = "sha256-/UjS44xgUdImccL+j+Yg2S9tZdvMl25oVS6YHQvpwLo=";
    };
  };
}
