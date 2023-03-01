
# Build and install
1. build release version of crate
    ```bash
    maturin build --release
    ```
    builded wheel will be at [./target/wheels](./target/wheels)
2. install builded wheel to local python
    ```bash
    # with pip
    pip install ./target/wheels/py_numass-0.10-cp311-cp311-manylinux_2_34_x86_64.whl

    # or with poetry
    poetry add ./target/wheels/py_numass-0.10-cp311-cp311-manylinux_2_34_x86_64.whl
    ```

# Developing
1. initialise poetry environment
    ```bash
    poetry shell
    ```
2. install dependencies (dfparser and ipython)
    ```bash
    poetry install
    ```
3. the crate as module in the current virtualenv
    ```bash
    maturin develop
    ```
4. run example script to validate crate
    ```bash
    python examples/process-point.py
    ```



