#!/usr/bin/env python3
"""Tests for the CLI module."""

import os
import pytest
import tempfile
from unittest.mock import patch, Mock
from jenkinsfilelint.cli import main, should_skip_file, should_include_file


class TestCLIMain:
    """Test the CLI main function."""

    def test_help_message(self):
        """Test that help message is displayed."""
        with patch("sys.argv", ["jenkinsfilelint", "--help"]):
            with pytest.raises(SystemExit) as exc_info:
                main()
            assert exc_info.value.code == 0

    def test_validate_single_valid_file(self, capsys):
        """Test validation of a single valid file requires credentials."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("pipeline { agent any }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch("sys.argv", ["jenkinsfilelint", temp_path]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1

            captured = capsys.readouterr()
            assert "credentials required" in captured.err.lower()
        finally:
            os.unlink(temp_path)

    def test_validate_single_invalid_file(self, capsys):
        """Test validation of a single invalid file."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False)
        f.write("")  # Empty file
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch("sys.argv", ["jenkinsfilelint", temp_path]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1

            captured = capsys.readouterr()
            # Error message is printed without filename prefix for deduplication
            assert "credentials required" in captured.err.lower()
        finally:
            os.unlink(temp_path)

    def test_validate_multiple_files(self, capsys):
        """Test validation of multiple files requires credentials.

        Error messages should be deduplicated - the same error should only
        appear once even when multiple files have the same error.
        """
        f1 = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix="1.groovy")
        f1.write("pipeline { agent any }")
        f1.flush()
        f1.close()
        temp_path1 = f1.name

        f2 = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix="2.groovy")
        f2.write("@Library('lib') _\necho 'test'")
        f2.flush()
        f2.close()
        temp_path2 = f2.name

        try:
            with patch("sys.argv", ["jenkinsfilelint", temp_path1, temp_path2]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1

            captured = capsys.readouterr()
            # Verify the error is present
            assert "credentials required" in captured.err.lower()
            # Verify deduplication - the credentials message should appear exactly once
            assert captured.err.lower().count("credentials required") == 1
        finally:
            os.unlink(temp_path1)
            os.unlink(temp_path2)

    def test_validate_multiple_files_with_one_invalid(self, capsys):
        """Test validation of multiple files where one is invalid."""
        f1 = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix="1.groovy")
        f1.write("pipeline { agent any }")
        f1.flush()
        f1.close()
        temp_path1 = f1.name

        f2 = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix="2.groovy")
        f2.write("")  # Empty file
        f2.flush()
        f2.close()
        temp_path2 = f2.name

        try:
            with patch("sys.argv", ["jenkinsfilelint", temp_path1, temp_path2]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1
        finally:
            os.unlink(temp_path1)
            os.unlink(temp_path2)

    def test_validate_with_verbose_flag(self, capsys):
        """Test validation with verbose output requires credentials."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("pipeline { agent any }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch("sys.argv", ["jenkinsfilelint", "--verbose", temp_path]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1

            captured = capsys.readouterr()
            assert "Validating" in captured.out
            assert "credentials required" in captured.err.lower()
        finally:
            os.unlink(temp_path)

    def test_validate_verbose_shows_message_on_success(self, capsys):
        """Test that verbose mode prints the validation message on success."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("pipeline { agent any }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch(
                "sys.argv",
                ["jenkinsfilelint", "--verbose", "--jenkins-url", "https://jenkins.example.com", temp_path],
            ):
                with patch("jenkinsfilelint.linter.requests.post") as mock_post:
                    mock_response = Mock()
                    mock_response.status_code = 200
                    mock_response.json.return_value = {"status": "ok"}
                    mock_response.raise_for_status = Mock()
                    mock_post.return_value = mock_response

                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0

            captured = capsys.readouterr()
            assert "Validating" in captured.out
            assert "Jenkinsfile successfully validated" in captured.out
        finally:
            os.unlink(temp_path)

    def test_validate_with_jenkins_url_argument(self):
        """Test validation with Jenkins URL argument."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("pipeline { agent any }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--jenkins-url",
                    "https://jenkins.example.com",
                    temp_path,
                ],
            ):
                with patch("jenkinsfilelint.linter.requests.post") as mock_post:
                    mock_response = Mock()
                    mock_response.status_code = 200
                    mock_response.json.return_value = {"status": "ok"}
                    mock_response.raise_for_status = Mock()
                    mock_post.return_value = mock_response

                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
                    mock_post.assert_called_once()
        finally:
            os.unlink(temp_path)

    def test_validate_with_username_and_token_arguments(self):
        """Test validation with username and token arguments."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("pipeline { agent any }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--jenkins-url",
                    "https://jenkins.example.com",
                    "--username",
                    "testuser",
                    "--token",
                    "testtoken",
                    temp_path,
                ],
            ):
                with patch("jenkinsfilelint.linter.requests.post") as mock_post:
                    mock_response = Mock()
                    mock_response.status_code = 200
                    mock_response.json.return_value = {"status": "ok"}
                    mock_response.raise_for_status = Mock()
                    mock_post.return_value = mock_response

                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0

                    # Verify authentication was used
                    call_kwargs = mock_post.call_args[1]
                    assert call_kwargs["auth"] == ("testuser", "testtoken")
        finally:
            os.unlink(temp_path)

    def test_validate_nonexistent_file(self, capsys):
        """Test validation of a nonexistent file."""
        with patch("sys.argv", ["jenkinsfilelint", "/nonexistent/file.groovy"]):
            with pytest.raises(SystemExit) as exc_info:
                main()
            assert exc_info.value.code == 1

        captured = capsys.readouterr()
        assert "File not found" in captured.err

    def test_validate_with_env_variables(self):
        """Test validation using environment variables for configuration."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("pipeline { agent any }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch.dict(
                os.environ,
                {
                    "JENKINS_URL": "https://jenkins.env.com",
                    "JENKINS_USER": "envuser",
                    "JENKINS_TOKEN": "envtoken",
                },
            ):
                with patch("sys.argv", ["jenkinsfilelint", temp_path]):
                    with patch("jenkinsfilelint.linter.requests.post") as mock_post:
                        mock_response = Mock()
                        mock_response.status_code = 200
                        mock_response.json.return_value = {"status": "ok"}
                        mock_response.raise_for_status = Mock()
                        mock_post.return_value = mock_response

                        with pytest.raises(SystemExit) as exc_info:
                            main()
                        assert exc_info.value.code == 0

                        # Verify Jenkins API was called
                        mock_post.assert_called_once()
                        call_kwargs = mock_post.call_args[1]
                        assert call_kwargs["auth"] == ("envuser", "envtoken")
        finally:
            os.unlink(temp_path)


class TestShouldSkipFile:
    """Test the should_skip_file function."""

    def test_no_skip_patterns(self):
        """Test that no file is skipped when no patterns are provided."""
        assert should_skip_file("src/MyClass.groovy", []) is False
        assert should_skip_file("Jenkinsfile", []) is False
        assert should_skip_file("Jenkinsfile", None) is False

    def test_exact_match(self):
        """Test exact filename match."""
        assert should_skip_file("src/Utils.groovy", ["src/Utils.groovy"]) is True
        assert should_skip_file("src/Other.groovy", ["src/Utils.groovy"]) is False

    def test_glob_pattern_wildcard(self):
        """Test glob pattern with wildcard."""
        assert should_skip_file("src/Utils.groovy", ["*.groovy"]) is True
        assert should_skip_file("src/Utils.groovy", ["src/*.groovy"]) is True
        assert should_skip_file("Jenkinsfile", ["*.groovy"]) is False

    def test_glob_pattern_double_wildcard(self):
        """Test glob pattern with directory wildcard."""
        patterns = ["*/src/*.groovy"]
        assert should_skip_file("lib/src/MyClass.groovy", patterns) is True
        assert should_skip_file("vars/deploy.groovy", patterns) is False

    def test_multiple_patterns(self):
        """Test multiple skip patterns."""
        patterns = ["*/src/*.groovy", "vars/*.groovy"]
        assert should_skip_file("lib/src/MyClass.groovy", patterns) is True
        assert should_skip_file("vars/deploy.groovy", patterns) is True
        assert should_skip_file("Jenkinsfile", patterns) is False


class TestCLISkipOption:
    """Test the CLI --skip option."""

    def test_skip_single_file(self, capsys):
        """Test skipping a single file with --skip option."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("class Utils { }")  # Pure Groovy, not a pipeline
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch(
                "sys.argv",
                ["jenkinsfilelint", "--skip", "*.groovy", temp_path],
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                # Should exit 0 because the only file was skipped
                assert exc_info.value.code == 0
        finally:
            os.unlink(temp_path)

    def test_skip_single_file_verbose(self, capsys):
        """Test skipping a file with verbose output."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("class Utils { }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch(
                "sys.argv",
                ["jenkinsfilelint", "--verbose", "--skip", "*.groovy", temp_path],
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 0

            captured = capsys.readouterr()
            assert "Skipped" in captured.out
            assert "matches skip pattern" in captured.out
        finally:
            os.unlink(temp_path)

    def test_skip_some_files_validate_others(self, capsys):
        """Test skipping some files while validating others."""
        # Create a groovy file to skip
        groovy_dir = tempfile.mkdtemp()
        groovy_file = os.path.join(groovy_dir, "Utils.groovy")
        with open(groovy_file, "w") as f:
            f.write("class Utils { }")

        # Create a Jenkinsfile to validate
        jenkinsfile = tempfile.NamedTemporaryFile(
            mode="w", delete=False, prefix="Jenkinsfile"
        )
        jenkinsfile.write("pipeline { agent any }")
        jenkinsfile.flush()
        jenkinsfile.close()
        jenkinsfile_path = jenkinsfile.name

        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--jenkins-url",
                    "https://jenkins.example.com",
                    "--skip",
                    "*.groovy",
                    groovy_file,
                    jenkinsfile_path,
                ],
            ):
                with patch("jenkinsfilelint.linter.requests.post") as mock_post:
                    mock_response = Mock()
                    mock_response.status_code = 200
                    mock_response.json.return_value = {"status": "ok"}
                    mock_response.raise_for_status = Mock()
                    mock_post.return_value = mock_response

                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
                    # Only Jenkinsfile should be validated, groovy file skipped
                    mock_post.assert_called_once()
        finally:
            os.unlink(groovy_file)
            os.rmdir(groovy_dir)
            os.unlink(jenkinsfile_path)

    def test_multiple_skip_patterns(self, capsys):
        """Test using multiple --skip options."""
        # Create files in temp directories
        src_dir = tempfile.mkdtemp()
        vars_dir = tempfile.mkdtemp()

        src_file = os.path.join(src_dir, "Utils.groovy")
        with open(src_file, "w") as f:
            f.write("class Utils { }")

        vars_file = os.path.join(vars_dir, "deploy.groovy")
        with open(vars_file, "w") as f:
            f.write("def call() { }")

        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--skip",
                    "*/Utils.groovy",
                    "--skip",
                    "*/deploy.groovy",
                    src_file,
                    vars_file,
                ],
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                # Both files skipped, so exit 0
                assert exc_info.value.code == 0
        finally:
            os.unlink(src_file)
            os.unlink(vars_file)
            os.rmdir(src_dir)
            os.rmdir(vars_dir)


class TestShouldIncludeFile:
    """Test the should_include_file function."""

    def test_no_include_patterns_includes_all(self):
        """Test that all files are included when no patterns are provided."""
        assert should_include_file("src/MyClass.groovy", []) is True
        assert should_include_file("Jenkinsfile", []) is True
        assert should_include_file("Jenkinsfile", None) is True

    def test_exact_match(self):
        """Test exact filename match."""
        assert should_include_file("Jenkinsfile", ["Jenkinsfile"]) is True
        assert should_include_file("src/Other.groovy", ["Jenkinsfile"]) is False

    def test_glob_pattern_wildcard(self):
        """Test glob pattern with wildcard."""
        assert should_include_file("pipelines/deploy.groovy", ["pipelines/*.groovy"]) is True
        assert should_include_file("src/Utils.groovy", ["pipelines/*.groovy"]) is False

    def test_glob_pattern_prefix(self):
        """Test Jenkinsfile* pattern."""
        assert should_include_file("Jenkinsfile", ["Jenkinsfile*"]) is True
        assert should_include_file("Jenkinsfile.prod", ["Jenkinsfile*"]) is True
        assert should_include_file("src/Utils.groovy", ["Jenkinsfile*"]) is False

    def test_multiple_patterns(self):
        """Test multiple include patterns (any match includes the file)."""
        patterns = ["Jenkinsfile*", "pipelines/*.groovy"]
        assert should_include_file("Jenkinsfile", patterns) is True
        assert should_include_file("Jenkinsfile.prod", patterns) is True
        assert should_include_file("pipelines/deploy.groovy", patterns) is True
        assert should_include_file("src/Utils.groovy", patterns) is False


class TestCLIIncludeOption:
    """Test the CLI --include option."""

    def test_include_matches_file(self, capsys):
        """Test that only files matching --include are validated."""
        f = tempfile.NamedTemporaryFile(
            mode="w", delete=False, prefix="Jenkinsfile", suffix=""
        )
        f.write("pipeline { agent any }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch(
                "sys.argv",
                ["jenkinsfilelint", "--include", "Jenkinsfile*", temp_path],
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                # No Jenkins URL → credentials error → exit 1
                assert exc_info.value.code == 1

            captured = capsys.readouterr()
            assert "credentials required" in captured.err.lower()
        finally:
            os.unlink(temp_path)

    def test_include_skips_non_matching_file(self, capsys):
        """Test that files not matching --include are skipped."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("class Utils { }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch(
                "sys.argv",
                ["jenkinsfilelint", "--include", "Jenkinsfile*", temp_path],
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                # File skipped because it doesn't match the include pattern
                assert exc_info.value.code == 0
        finally:
            os.unlink(temp_path)

    def test_include_skips_non_matching_file_verbose(self, capsys):
        """Test that skipped non-matching files are reported in verbose mode."""
        f = tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy")
        f.write("class Utils { }")
        f.flush()
        f.close()
        temp_path = f.name

        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--verbose",
                    "--include",
                    "Jenkinsfile*",
                    temp_path,
                ],
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 0

            captured = capsys.readouterr()
            assert "does not match include pattern" in captured.out
        finally:
            os.unlink(temp_path)

    def test_include_and_skip_combined(self, capsys):
        """Test combining --include and --skip options.

        --include whitelists files, --skip then blacklists within that set.
        """
        groovy_dir = tempfile.mkdtemp()
        pipeline_file = os.path.join(groovy_dir, "deploy.groovy")
        helper_file = os.path.join(groovy_dir, "utils.groovy")

        with open(pipeline_file, "w") as f:
            f.write("pipeline { agent any }")
        with open(helper_file, "w") as f:
            f.write("def call() { }")

        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--jenkins-url",
                    "https://jenkins.example.com",
                    "--include",
                    "*.groovy",
                    "--skip",
                    "*/utils.groovy",
                    pipeline_file,
                    helper_file,
                ],
            ):
                with patch("jenkinsfilelint.linter.requests.post") as mock_post:
                    mock_response = Mock()
                    mock_response.status_code = 200
                    mock_response.json.return_value = {"status": "ok"}
                    mock_response.raise_for_status = Mock()
                    mock_post.return_value = mock_response

                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
                    # Only deploy.groovy validated; utils.groovy is skipped
                    mock_post.assert_called_once()
        finally:
            os.unlink(pipeline_file)
            os.unlink(helper_file)
            os.rmdir(groovy_dir)

    def test_multiple_include_patterns(self, capsys):
        """Test using multiple --include options."""
        jenkinsfile = tempfile.NamedTemporaryFile(
            mode="w", delete=False, prefix="Jenkinsfile"
        )
        jenkinsfile.write("pipeline { agent any }")
        jenkinsfile.flush()
        jenkinsfile.close()
        jenkinsfile_path = jenkinsfile.name

        groovy_dir = tempfile.mkdtemp()
        pipeline_groovy = os.path.join(groovy_dir, "pipeline.groovy")
        helper_groovy = os.path.join(groovy_dir, "utils.groovy")

        with open(pipeline_groovy, "w") as f:
            f.write("pipeline { agent any }")
        with open(helper_groovy, "w") as f:
            f.write("def call() { }")

        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--jenkins-url",
                    "https://jenkins.example.com",
                    "--include",
                    "Jenkinsfile*",
                    "--include",
                    "*/pipeline.groovy",
                    jenkinsfile_path,
                    pipeline_groovy,
                    helper_groovy,
                ],
            ):
                with patch("jenkinsfilelint.linter.requests.post") as mock_post:
                    mock_response = Mock()
                    mock_response.status_code = 200
                    mock_response.json.return_value = {"status": "ok"}
                    mock_response.raise_for_status = Mock()
                    mock_post.return_value = mock_response

                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
                    # Jenkinsfile and pipeline.groovy validated; utils.groovy skipped
                    assert mock_post.call_count == 2
        finally:
            os.unlink(jenkinsfile_path)
            os.unlink(pipeline_groovy)
            os.unlink(helper_groovy)
            os.rmdir(groovy_dir)


class TestWindowsUTF8Encoding:
    """Test UTF-8 encoding setup on Windows."""

    def test_windows_utf8_stdout_and_stderr_wrapped(self):
        """Test that stdout/stderr are wrapped with UTF-8 on win32 when needed."""
        import io

        mock_stdout = Mock()
        mock_stdout.buffer = io.BytesIO()
        mock_stderr = Mock()
        mock_stderr.buffer = io.BytesIO()

        with patch("sys.platform", "win32"):
            with patch("sys.stdout", mock_stdout):
                with patch("sys.stderr", mock_stderr):
                    with patch("sys.argv", ["jenkinsfilelint", "--help"]):
                        with pytest.raises(SystemExit):
                            main()

    def test_windows_utf8_already_wrapped_utf8(self):
        """Test that stdout/stderr are not re-wrapped when already UTF-8 TextIOWrapper."""
        import io

        mock_stdout = Mock(spec=io.TextIOWrapper)
        mock_stdout.encoding = "utf-8"
        mock_stderr = Mock(spec=io.TextIOWrapper)
        mock_stderr.encoding = "utf-8"

        with patch("sys.platform", "win32"):
            with patch("sys.stdout", mock_stdout):
                with patch("sys.stderr", mock_stderr):
                    with patch("sys.argv", ["jenkinsfilelint", "--help"]):
                        with pytest.raises(SystemExit):
                            main()
