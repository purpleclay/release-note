# Release Note

Generate a release note for your project.

## Install

To install the latest version using a bash script:

```sh
sh -c "$(curl https://raw.githubusercontent.com/purpleclay/release-note/main/scripts/install.sh)"
```

Download a specific version using the `-v` flag. The script uses `sudo` by default but can be disabled through the `--no-sudo` flag. You can also provide a different installation directory from the default `/usr/local/bin` by using the `-d` flag:

```sh
sh -c "$(curl https://raw.githubusercontent.com/purpleclay/release-note/main/scripts/install.sh)" \
  -- -v 0.7.0 --no-sudo -d ./bin
```

## Run with Nix

If you have nix installed, you can run the binary directly from the GitHub repository:

```sh
nix run github:purpleclay/release-note -- --help
```

## Verifying a release

Every release archive (and `checksums.txt`) is a subject of a SLSA build provenance attestation, signed by the [release-workflows](https://github.com/purpleclay/release-workflows) reusable workflow that built it:

```sh
gh attestation verify <archive>.tar.gz \
  --repo purpleclay/release-note \
  --signer-workflow purpleclay/release-workflows/.github/workflows/release-rust.yml
```

The `--signer-workflow` check confirms the signing identity belongs to that reusable workflow, not this repository directly — the SLSA Build L3 claim.
