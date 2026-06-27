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
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", p]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1
            captured = capsys.readouterr()
            assert "jenkins url not provided" in captured.err.lower()
        finally:
            os.unlink(p)

    def test_validate_single_invalid_file(self, capsys):
        """Test validation of a single invalid file."""
        with tempfile.NamedTemporaryFile(mode="w", delete=False) as f:
            f.write("")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", p]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1
            captured = capsys.readouterr()
            assert "jenkins url not provided" in captured.err.lower()
        finally:
            os.unlink(p)

    def test_validate_multiple_files(self, capsys):
        """Test validation of multiple files — error deduplication."""
        with tempfile.NamedTemporaryFile(
            mode="w", delete=False, suffix="1.groovy"
        ) as f:
            f.write("pipeline { agent any }")
            p1 = f.name
        with tempfile.NamedTemporaryFile(
            mode="w", delete=False, suffix="2.groovy"
        ) as f:
            f.write("@Library('lib') _\necho 'test'")
            p2 = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", p1, p2]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1
            captured = capsys.readouterr()
            assert captured.err.lower().count("jenkins url not provided") == 1
        finally:
            os.unlink(p1)
            os.unlink(p2)

    def test_validate_multiple_files_with_one_invalid(self):
        """Test validation of multiple files where one is invalid."""
        with tempfile.NamedTemporaryFile(
            mode="w", delete=False, suffix="1.groovy"
        ) as f:
            f.write("pipeline { agent any }")
            p1 = f.name
        with tempfile.NamedTemporaryFile(
            mode="w", delete=False, suffix="2.groovy"
        ) as f:
            f.write("")
            p2 = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", p1, p2]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1
        finally:
            os.unlink(p1)
            os.unlink(p2)

    def test_validate_with_verbose_flag(self, capsys):
        """Test validation with verbose output."""
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", "--verbose", p]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1
            captured = capsys.readouterr()
            assert "Validating" in captured.out
            assert "jenkins url not provided" in captured.err.lower()
        finally:
            os.unlink(p)

    def test_validate_verbose_shows_message_on_success(self, capsys):
        """Test verbose mode prints the validation message on success."""
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--verbose",
                    "--jenkins-url",
                    "https://jenkins.example.com",
                    p,
                ],
            ):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "Jenkinsfile successfully validated")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
            captured = capsys.readouterr()
            assert "Validating" in captured.out
            assert "successfully validated" in captured.out
        finally:
            os.unlink(p)

    def test_validate_with_jenkins_url_argument(self):
        """Test validation with --jenkins-url argument."""
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--jenkins-url",
                    "https://jenkins.example.com",
                    p,
                ],
            ):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "ok")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
        finally:
            os.unlink(p)

    def test_validate_with_username_and_token_arguments(self):
        """Test validation with --username and --token arguments."""
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
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
                    p,
                ],
            ):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "ok")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
        finally:
            os.unlink(p)

    def test_validate_nonexistent_file(self, capsys):
        """Test validation of a nonexistent file."""
        with patch("sys.argv", ["jenkinsfilelint", "/nonexistent/file.groovy"]):
            with pytest.raises(SystemExit) as exc_info:
                main()
            assert exc_info.value.code == 1
        captured = capsys.readouterr()
        assert "File not found" in captured.err

    def test_validate_with_env_variables(self):
        """Test validation using environment variables."""
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch.dict(
                os.environ,
                {
                    "JENKINS_URL": "https://jenkins.env.com",
                    "JENKINS_USER": "envuser",
                    "JENKINS_TOKEN": "envtoken",
                },
            ):
                with patch("sys.argv", ["jenkinsfilelint", p]):
                    with patch(
                        "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                    ) as mock_val:
                        mock_val.return_value = (True, "ok")
                        with pytest.raises(SystemExit) as exc_info:
                            main()
                        assert exc_info.value.code == 0
        finally:
            os.unlink(p)


class TestShouldSkipFile:
    """Test the should_skip_file function."""

    def test_no_skip_patterns(self):
        assert should_skip_file("src/MyClass.groovy", []) is False
        assert should_skip_file("Jenkinsfile", None) is False

    def test_exact_match(self):
        assert should_skip_file("src/Utils.groovy", ["src/Utils.groovy"]) is True

    def test_glob_pattern_wildcard(self):
        assert should_skip_file("src/Utils.groovy", ["*.groovy"]) is True
        assert should_skip_file("Jenkinsfile", ["*.groovy"]) is False

    def test_glob_pattern_double_wildcard(self):
        assert should_skip_file("lib/src/MyClass.groovy", ["*/src/*.groovy"]) is True

    def test_multiple_patterns(self):
        patterns = ["*/src/*.groovy", "vars/*.groovy"]
        assert should_skip_file("lib/src/MyClass.groovy", patterns) is True
        assert should_skip_file("Jenkinsfile", patterns) is False


class TestCLISkipOption:
    """Test the CLI --skip option."""

    def test_skip_single_file(self):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("class Utils { }")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", "--skip", "*.groovy", p]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 0
        finally:
            os.unlink(p)

    def test_skip_single_file_verbose(self, capsys):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("class Utils { }")
            p = f.name
        try:
            with patch(
                "sys.argv", ["jenkinsfilelint", "--verbose", "--skip", "*.groovy", p]
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 0
            captured = capsys.readouterr()
            assert "Skipped" in captured.out
        finally:
            os.unlink(p)

    def test_skip_some_files_validate_others(self, capsys):
        """Skip some files, validate others."""
        import tempfile as tf

        dir = tf.mkdtemp()
        groovy = os.path.join(dir, "Utils.groovy")
        with open(groovy, "w") as f:
            f.write("class Utils { }")
        with tempfile.NamedTemporaryFile(
            mode="w", delete=False, prefix="Jenkinsfile"
        ) as f:
            f.write("pipeline { agent any }")
            jp = f.name
        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--jenkins-url",
                    "https://jenkins.example.com",
                    "--skip",
                    "*.groovy",
                    groovy,
                    jp,
                ],
            ):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "ok")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
                    mock_val.assert_called_once_with(jp)
        finally:
            os.unlink(groovy)
            os.rmdir(dir)
            os.unlink(jp)

    def test_multiple_skip_patterns(self):
        dir1 = tempfile.mkdtemp()
        dir2 = tempfile.mkdtemp()
        f1 = os.path.join(dir1, "Utils.groovy")
        f2 = os.path.join(dir2, "deploy.groovy")
        for f in (f1, f2):
            with open(f, "w") as fh:
                fh.write("def call() { }")
        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--skip",
                    "*/Utils.groovy",
                    "--skip",
                    "*/deploy.groovy",
                    f1,
                    f2,
                ],
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 0
        finally:
            os.unlink(f1)
            os.unlink(f2)
            os.rmdir(dir1)
            os.rmdir(dir2)


class TestShouldIncludeFile:
    """Test the should_include_file function."""

    def test_no_include_patterns_includes_all(self):
        assert should_include_file("Jenkinsfile", []) is True
        assert should_include_file("Jenkinsfile", None) is True

    def test_exact_match(self):
        assert should_include_file("Jenkinsfile", ["Jenkinsfile"]) is True
        assert should_include_file("src/Other.groovy", ["Jenkinsfile"]) is False

    def test_glob_pattern_wildcard(self):
        assert (
            should_include_file("pipelines/deploy.groovy", ["pipelines/*.groovy"])
            is True
        )

    def test_glob_pattern_prefix(self):
        assert should_include_file("Jenkinsfile.prod", ["Jenkinsfile*"]) is True

    def test_multiple_patterns(self):
        patterns = ["Jenkinsfile*", "pipelines/*.groovy"]
        assert should_include_file("Jenkinsfile", patterns) is True
        assert should_include_file("src/Utils.groovy", patterns) is False


class TestCLIIncludeOption:
    """Test the CLI --include option."""

    def test_include_matches_file(self, capsys):
        with tempfile.NamedTemporaryFile(
            mode="w", delete=False, prefix="Jenkinsfile"
        ) as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", "--include", "Jenkinsfile*", p]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1
            assert "jenkins url not provided" in capsys.readouterr().err.lower()
        finally:
            os.unlink(p)

    def test_include_skips_non_matching_file(self, capsys):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("class Utils { }")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", "--include", "Jenkinsfile*", p]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 0
        finally:
            os.unlink(p)

    def test_include_skips_non_matching_file_verbose(self, capsys):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("class Utils { }")
            p = f.name
        try:
            with patch(
                "sys.argv",
                ["jenkinsfilelint", "--verbose", "--include", "Jenkinsfile*", p],
            ):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 0
            assert "does not match include pattern" in capsys.readouterr().out
        finally:
            os.unlink(p)

    def test_include_and_skip_combined(self):
        dir = tempfile.mkdtemp()
        pf = os.path.join(dir, "deploy.groovy")
        hf = os.path.join(dir, "utils.groovy")
        with open(pf, "w") as f:
            f.write("pipeline { agent any }")
        with open(hf, "w") as f:
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
                    pf,
                    hf,
                ],
            ):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "ok")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
                    mock_val.assert_called_once_with(pf)
        finally:
            os.unlink(pf)
            os.unlink(hf)
            os.rmdir(dir)

    def test_multiple_include_patterns(self):
        with tempfile.NamedTemporaryFile(
            mode="w", delete=False, prefix="Jenkinsfile"
        ) as f:
            f.write("pipeline { agent any }")
            jp = f.name
        dir = tempfile.mkdtemp()
        pg = os.path.join(dir, "pipeline.groovy")
        ug = os.path.join(dir, "utils.groovy")
        with open(pg, "w") as f:
            f.write("pipeline { agent any }")
        with open(ug, "w") as f:
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
                    jp,
                    pg,
                    ug,
                ],
            ):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "ok")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
                    assert mock_val.call_count == 2
        finally:
            os.unlink(jp)
            os.unlink(pg)
            os.unlink(ug)
            os.rmdir(dir)


class TestCLIRunnerOption:
    """Test the CLI --runner option."""

    def test_docker_runner_valid_choice(self):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", "--runner", "docker", p]):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "ok")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
        finally:
            os.unlink(p)

    def test_docker_runner_failure_exits_nonzero(self, capsys):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent }")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", "--runner", "docker", p]):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (
                        False,
                        "Validation errors:\nunexpected token",
                    )
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 1
            captured = capsys.readouterr()
            assert "unexpected token" in captured.err
        finally:
            os.unlink(p)

    def test_docker_runner_verbose_shows_progress(self, capsys):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch(
                "sys.argv",
                ["jenkinsfilelint", "--verbose", "--runner", "docker", p],
            ):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "Jenkinsfile successfully validated")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
            captured = capsys.readouterr()
            assert "Validating" in captured.out
            assert "successfully validated" in captured.out
        finally:
            os.unlink(p)

    def test_docker_runner_help_message(self, capsys):
        with patch("sys.argv", ["jenkinsfilelint", "--help"]):
            with pytest.raises(SystemExit) as exc_info:
                main()
            assert exc_info.value.code == 0
        captured = capsys.readouterr()
        assert "--runner" in captured.out
        assert "jenkins" in captured.out
        assert "docker" in captured.out

    def test_docker_runner_invalid_choice(self):
        with patch(
            "sys.argv", ["jenkinsfilelint", "--runner", "invalid", "Jenkinsfile"]
        ):
            with pytest.raises(SystemExit) as exc_info:
                main()
            assert exc_info.value.code == 2

    def test_jenkins_runner_default(self, capsys):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch("sys.argv", ["jenkinsfilelint", p]):
                with pytest.raises(SystemExit) as exc_info:
                    main()
                assert exc_info.value.code == 1
            assert "jenkins url not provided" in capsys.readouterr().err.lower()
        finally:
            os.unlink(p)

    def test_runner_env_var_not_overridden(self):
        with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".groovy") as f:
            f.write("pipeline { agent any }")
            p = f.name
        try:
            with patch(
                "sys.argv",
                [
                    "jenkinsfilelint",
                    "--runner",
                    "docker",
                    "--jenkins-url",
                    "https://example.com",
                    p,
                ],
            ):
                with patch(
                    "jenkinsfilelint.linter.JenkinsfileLinter.validate"
                ) as mock_val:
                    mock_val.return_value = (True, "ok")
                    with pytest.raises(SystemExit) as exc_info:
                        main()
                    assert exc_info.value.code == 0
        finally:
            os.unlink(p)


class TestWindowsUTF8Encoding:
    """Test UTF-8 encoding setup on Windows."""

    def test_windows_utf8_stdout_and_stderr_wrapped(self):
        import io

        mock_stdout = Mock(spec=io.IOBase)
        mock_stdout.buffer = io.BytesIO()
        mock_stderr = Mock(spec=io.IOBase)
        mock_stderr.buffer = io.BytesIO()
        # Make them look like not already UTF-8 TextIOWrapper
        with patch("sys.platform", "win32"):
            with patch("sys.stdout", mock_stdout):
                with patch("sys.stderr", mock_stderr):
                    with patch("sys.argv", ["jenkinsfilelint", "--help"]):
                        with pytest.raises(SystemExit):
                            main()

    def test_windows_utf8_already_wrapped_utf8(self):
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
