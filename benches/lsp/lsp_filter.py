#!/usr/bin/env python3
"""Adapt Solar's initialization and indexing signals for lsp-bench v0.3.3."""

from __future__ import annotations

import json
import subprocess
import sys
import threading
from pathlib import Path
from typing import BinaryIO


SYNTHETIC_PROGRESS_END = json.dumps(
    {
        "jsonrpc": "2.0",
        "method": "$/progress",
        "params": {
            "token": "solar-lsp-bench-index",
            "value": {"kind": "end"},
        },
    },
    separators=(",", ":"),
).encode()


def validate_server_message(message: object) -> dict[str, object]:
    if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
        raise RuntimeError("server message is not a JSON-RPC 2.0 object")

    if "method" in message:
        allowed = {"jsonrpc", "method", "params", "id"}
        request_id = message.get("id")
        if (
            not set(message) <= allowed
            or not isinstance(message.get("method"), str)
            or not message["method"]
            or (
                "id" in message
                and (
                    isinstance(request_id, bool)
                    or not isinstance(request_id, (int, str))
                )
            )
            or (
                "params" in message
                and not isinstance(message["params"], (dict, list))
            )
        ):
            raise RuntimeError("server message is not a JSON-RPC request or notification")
        return message

    request_id = message.get("id")
    has_result = "result" in message
    has_error = "error" in message
    if (
        "id" not in message
        or isinstance(request_id, bool)
        or (
            request_id is not None
            and not isinstance(request_id, (int, str))
        )
        or has_result == has_error
    ):
        raise RuntimeError("server message is not a JSON-RPC response")
    expected = {"jsonrpc", "id", "result" if has_result else "error"}
    if set(message) != expected:
        raise RuntimeError("server message is not a JSON-RPC response")
    if has_error:
        error = message["error"]
        if (
            not isinstance(error, dict)
            or not {"code", "message"} <= set(error) <= {"code", "message", "data"}
            or isinstance(error.get("code"), bool)
            or not isinstance(error.get("code"), int)
            or not isinstance(error.get("message"), str)
        ):
            raise RuntimeError("server message has an invalid JSON-RPC error")
    return message


class NotificationFilter:
    """Validate initialization and select the fixture's diagnostics notification."""

    def __init__(self) -> None:
        self.initialize_request_id: int | str | None = None
        self.awaiting_diagnostics = False
        self.fixture_uri: str | None = None
        self.deferred_progress_end: bytes | None = None

    def observe_client(self, message: object) -> None:
        if not isinstance(message, dict):
            return
        method = message.get("method")
        request_id = message.get("id")
        if (
            method == "initialize"
            and isinstance(request_id, (int, str))
            and not isinstance(request_id, bool)
        ):
            self.initialize_request_id = message["id"]
        elif method == "initialized":
            self.awaiting_diagnostics = True
        elif method == "textDocument/didOpen":
            params = message.get("params")
            text_document = (
                params.get("textDocument") if isinstance(params, dict) else None
            )
            uri = (
                text_document.get("uri") if isinstance(text_document, dict) else None
            )
            if (
                self.fixture_uri is None
                and isinstance(uri, str)
                and uri.rsplit("/", 1)[-1] == "Main.sol"
            ):
                self.fixture_uri = uri

    def server_messages(self, message: object, body: bytes) -> list[bytes]:
        message = validate_server_message(message)
        if (
            self.initialize_request_id is not None
            and message.get("id") == self.initialize_request_id
            and "method" not in message
        ):
            self.initialize_request_id = None
            result = message.get("result")
            if (
                "error" in message
                or not isinstance(result, dict)
                or not isinstance(result.get("capabilities"), dict)
            ):
                return []
            return [body]
        method = message.get("method")
        if method == "$/progress" and "id" not in message:
            params = message.get("params")
            if not isinstance(params, dict) or not isinstance(
                params.get("value"), dict
            ):
                raise RuntimeError("progress notification has invalid params")
            kind = params["value"].get("kind")
            if not self.awaiting_diagnostics:
                return [body]
            if kind == "end":
                self.deferred_progress_end = body
            return []
        if not self.awaiting_diagnostics:
            return [body]
        if not isinstance(method, str) or "id" in message:
            return [body]
        if method == "textDocument/publishDiagnostics":
            params = message.get("params", {})
            uri = params.get("uri") if isinstance(params, dict) else None
            diagnostics = (
                params.get("diagnostics") if isinstance(params, dict) else None
            )
            has_fixture_warning = isinstance(diagnostics, list) and any(
                isinstance(diagnostic, dict)
                and str(diagnostic.get("code")) == "2018"
                and diagnostic.get("severity") == 2
                and diagnostic.get("message")
                == "function state mutability can be restricted to view"
                for diagnostic in diagnostics
            )
            if (
                isinstance(uri, str)
                and uri == self.fixture_uri
                and has_fixture_warning
            ):
                self.awaiting_diagnostics = False
                # Diagnostics mean Solar's initial analysis is ready; v0.3.3 also waits for progress.
                progress_end = self.deferred_progress_end or SYNTHETIC_PROGRESS_END
                self.deferred_progress_end = None
                output = [body, progress_end]
                return output
        return []


def read_message(stream: BinaryIO) -> bytes | None:
    content_length: int | None = None
    while True:
        line = stream.readline()
        if not line:
            return None
        if line in (b"\n", b"\r\n"):
            break
        name, separator, value = line.partition(b":")
        if not separator:
            raise RuntimeError("invalid LSP header")
        if name.strip().lower() == b"content-length":
            content_length = int(value.strip())
    if content_length is None or content_length < 0:
        raise RuntimeError("missing LSP content length")
    body = stream.read(content_length)
    if len(body) != content_length:
        raise RuntimeError("truncated LSP message")
    return body


def write_message(stream: BinaryIO, body: bytes) -> None:
    stream.write(f"Content-Length: {len(body)}\r\n\r\n".encode())
    stream.write(body)
    stream.flush()


def _reject_json_constant(value: str) -> None:
    raise ValueError(f"invalid JSON constant {value}")


def _reject_duplicate_keys(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise ValueError(f"duplicate JSON key {key}")
        result[key] = value
    return result


def parse_message(body: bytes) -> object:
    try:
        return json.loads(
            body,
            parse_constant=_reject_json_constant,
            object_pairs_hook=_reject_duplicate_keys,
        )
    except (UnicodeDecodeError, ValueError) as error:
        raise RuntimeError("message body is not valid strict JSON") from error


def forward_client(child: subprocess.Popen[bytes], notification_filter: NotificationFilter) -> None:
    assert child.stdin is not None
    try:
        while (body := read_message(sys.stdin.buffer)) is not None:
            message = parse_message(body)
            notification_filter.observe_client(message)
            write_message(child.stdin, body)
    finally:
        child.stdin.close()


def proxy(command: list[str]) -> int:
    child = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,
    )
    notification_filter = NotificationFilter()
    client_thread = threading.Thread(
        target=forward_client,
        args=(child, notification_filter),
        daemon=True,
    )
    client_thread.start()
    assert child.stdout is not None
    try:
        while (body := read_message(child.stdout)) is not None:
            message = parse_message(body)
            output = notification_filter.server_messages(message, body)
            for forwarded in output:
                write_message(sys.stdout.buffer, forwarded)
        return child.wait()
    finally:
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()


def main(arguments: list[str]) -> int:
    if not arguments:
        print("usage: lsp_filter.py SERVER [ARGS...]", file=sys.stderr)
        return 2
    server = Path(arguments[0]).resolve()
    if not server.is_file():
        print("server executable does not exist", file=sys.stderr)
        return 2
    return proxy([str(server), *arguments[1:]])


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
