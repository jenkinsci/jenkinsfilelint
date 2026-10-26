#!/usr/bin/env python3
"""Pytest configuration and fixtures."""

import pytest
import tempfile
import os


@pytest.fixture(autouse=True)
def clean_environment():
    """Clean Jenkins-related environment variables for all tests."""
    jenkins_vars = ["JENKINS_URL", "JENKINS_USER", "JENKINS_TOKEN"]
    # Save the original values
    original_values = {k: os.environ.get(k) for k in jenkins_vars}

    # Remove Jenkins variables
    for var in jenkins_vars:
        os.environ.pop(var, None)

    yield

    # Restore original values
    for k, v in original_values.items():
        if v is not None:
            os.environ[k] = v
        else:
            os.environ.pop(k, None)


@pytest.fixture
def valid_jenkinsfile():
    """Create a temporary valid Jenkinsfile."""
    f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
    f.write(
        """
pipeline {
    agent any
    stages {
        stage('Build') {
            steps {
                sh 'echo Building'
            }
        }
        stage('Test') {
            steps {
                sh 'echo Testing'
            }
        }
    }
}
"""
    )
    f.flush()
    f.close()
    temp_path = f.name

    yield temp_path
    os.unlink(temp_path)


@pytest.fixture
def empty_jenkinsfile():
    """Create a temporary empty Jenkinsfile."""
    f = tempfile.NamedTemporaryFile(mode="w", delete=False)
    f.write("")
    f.flush()
    f.close()
    temp_path = f.name

    yield temp_path
    os.unlink(temp_path)


@pytest.fixture
def invalid_jenkinsfile():
    """Create a temporary invalid Jenkinsfile (no pipeline declaration)."""
    f = tempfile.NamedTemporaryFile(mode="w", delete=False)
    f.write("// Just a comment\necho 'hello world'")
    f.flush()
    f.close()
    temp_path = f.name

    yield temp_path
    os.unlink(temp_path)


@pytest.fixture
def library_jenkinsfile():
    """Create a temporary Jenkinsfile with @Library declaration."""
    f = tempfile.NamedTemporaryFile(mode="w", delete=False)
    f.write(
        """
@Library('my-shared-library') _

myCustomFunction()
"""
    )
    f.flush()
    f.close()
    temp_path = f.name

    yield temp_path
    os.unlink(temp_path)


@pytest.fixture
def validate_server(monkeypatch):
    """A local stand-in for Jenkins's validate endpoint that counts TCP connections.

    ``server.connections`` is the number of connections opened so far and
    ``server.requests`` the number of files posted, so a test can tell whether
    the client reused its connections.
    """
    import threading
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    class Handler(BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"  # keep-alive

        def setup(self):
            super().setup()
            with server.lock:
                server.connections += 1

        def do_POST(self):
            self.rfile.read(int(self.headers["Content-Length"]))
            with server.lock:
                server.requests += 1
            body = b"Jenkinsfile successfully validated.\n"
            self.send_response(200)
            self.send_header("Content-Type", "text/plain;charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, *args):
            pass

    # Don't let a developer's proxy settings route the requests elsewhere.
    for var in ("NO_PROXY", "no_proxy"):
        monkeypatch.setenv(var, "127.0.0.1")

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    server.daemon_threads = True
    server.connections = 0
    server.requests = 0
    server.lock = threading.Lock()
    server.url = f"http://127.0.0.1:{server.server_address[1]}"
    thread = threading.Thread(
        target=server.serve_forever, kwargs={"poll_interval": 0.05}, daemon=True
    )
    thread.start()

    yield server

    server.shutdown()
    server.server_close()
