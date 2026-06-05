#!/usr/bin/env python3
"""Shared test fixtures."""

import pytest
from unittest.mock import patch, Mock


@pytest.fixture(autouse=True)
def mock_session_get():
    """Mock requests.Session.get for all tests to prevent real HTTP calls.

    By default, returns a valid crumb response so that crumb fetching
    doesn't interfere with tests that focus on validation logic.
    Tests that need different crumb behavior can override this fixture
    or add their own @patch("requests.Session.get").
    """
    with patch("requests.Session.get") as mock_get:
        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.json.return_value = {
            "crumb": "mock-crumb",
            "crumbRequestField": "Jenkins-Crumb",
        }
        mock_response.raise_for_status = Mock()
        mock_get.return_value = mock_response
        yield mock_get
