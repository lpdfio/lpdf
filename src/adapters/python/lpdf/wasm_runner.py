import json
import subprocess

from .exceptions import LpdfRenderError


class WasmRunner:
    def __init__(self, wasm_binary: str, wasm_runner: str = "wasmtime", timeout: int = 30):
        self._binary = wasm_binary
        self._runner = wasm_runner
        self._timeout = timeout

    def invoke(self, payload: dict) -> dict:
        try:
            result = subprocess.run(
                [self._runner, "run", self._binary],
                input=json.dumps(payload).encode(),
                capture_output=True,
                timeout=self._timeout,
            )
        except subprocess.TimeoutExpired:
            raise LpdfRenderError(
                f"WASI process timed out after {self._timeout} seconds."
            )
        if result.returncode != 0 or not result.stdout:
            raise LpdfRenderError(
                f"WASI process failed. Stderr: {result.stderr.decode()}"
            )
        response = json.loads(result.stdout)
        if "error" in response:
            raise LpdfRenderError(f"lpdf render error: {response['error']}")
        return response
