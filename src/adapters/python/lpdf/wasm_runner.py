import json
import subprocess


class WasmRunner:
    def __init__(self, wasm_binary: str, wasm_runner: str = "wasmtime"):
        self._binary = wasm_binary
        self._runner = wasm_runner

    def invoke(self, payload: dict) -> dict:
        result = subprocess.run(
            [self._runner, "run", self._binary],
            input=json.dumps(payload).encode(),
            capture_output=True,
        )
        if result.returncode != 0 or not result.stdout:
            raise RuntimeError(
                f"WASI process failed. Stderr: {result.stderr.decode()}"
            )
        response = json.loads(result.stdout)
        if "error" in response:
            raise RuntimeError(f"lpdf render error: {response['error']}")
        return response
