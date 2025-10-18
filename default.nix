{
  lib,
  openssl,
  pkg-config,
  rustPlatform,
  zlib,
}:
rustPlatform.buildRustPackage {
  pname = "release-note";
  version = (builtins.fromTOML (builtins.readFile ./Cargo.toml)).package.version;
  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = [
    pkg-config
  ];

  buildInputs = [
    openssl
    zlib
  ];

  meta = with lib; {
    homepage = "https://github.com/purpleclay/release-note";
    changelog = "https://github.com/purpleclay/release-note/releases/tag/${version}";
    description = "Generate a release note for your project";
    license = licenses.mit;
    maintainers = with maintainers; [purpleclay];
  };

  doCheck = false;
}
