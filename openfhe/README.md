# OpenFHE runtime

One pinned native installation serves all CBPIR crates:

- OpenFHE-rs `3e6a46b1377e89e09414cbca7c0c0140c5338397`
- OpenFHE 1.5.1 `1306d14f8c26bb6150d3e6ad54f28dfe1007689e`

From repository root:

```bash
OPENFHE_BUILD_JOBS=1 ./openfhe/bootstrap_openfhe.sh
export OPENFHE_INSTALL_DIR="$PWD/openfhe/.openfhe/openfhe-1.5.1/install"
export LD_LIBRARY_PATH="$OPENFHE_INSTALL_DIR/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

Install goes under ignored `openfhe/.openfhe/`. Search uses CKKS complex slots;
Fetch uses BGV packed integer slots. They do not share parameters or keys.
