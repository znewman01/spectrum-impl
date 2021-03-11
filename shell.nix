with import <nixpkgs> { };

let
  rust_overlay = import (builtins.fetchTarball
    "https://github.com/oxalica/rust-overlay/archive/master.tar.gz");
  nixpkgs = import <nixpkgs> { overlays = [ rust_overlay ]; };
  rustChannel = nixpkgs.rust-bin.nightly."2021-02-26".rust.override {
    extensions =
      [ "rust-src" "rust-analysis" "clippy-preview" "rustfmt-preview" ];
  };
  pythonPackages = python38Packages;
in pkgs.mkShell rec {
  name = "spectrum-env";
  venvDir = "./.venv";
  buildInputs = [
    # Basic build requirements
    gmp6
    stdenv
    rustChannel
    protobuf
    glibc
    gnum4
    openssl
    libffi
    pkgconfig
    # local testing
    etcd
    gnuplot

    # DevOps -- for running experiments
    packer
    terraform

    # Python -- for experiment scripts
    pythonPackages.python
    # We need a venv because some Python dependencies aren't in nixpkgs.
    # This execute some shell code to initialize a venv in $venvDir before
    # dropping into the shell
    pythonPackages.venvShellHook
    # Linting + development
    pythonPackages.black
    # pythonPackages.pylint
    pythonPackages.ipython
    nodePackages.pyright

    # experiment scripts
    awscli2
    curl
    tokei
  ];

  PROTOC = "${pkgs.protobuf}/bin/protoc";
  RUST_SRC_PATH = "${rustChannel}/lib/rustlib/src/rust/library";

  # Run this command, only after creating the virtual environment
  postVenvCreation = ''
    unset SOURCE_DATE_EPOCH
    pip install -r experiments/requirements.txt
    pip install pylint
  '';

  postShellHook = ''
    export PATH="$PATH:/home/zjn/.cargo/bin"
    # allow pip to install wheels
    unset SOURCE_DATE_EPOCH
  '';

}
